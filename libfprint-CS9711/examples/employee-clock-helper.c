/*
 * Employee clock helper for the Hopkins Inventory Management app.
 *
 * This stores serialized libfprint templates in an application directory and
 * identifies a scanned finger against that gallery, without using fprintd/PAM.
 */

#define FP_COMPONENT "employee-clock-helper"

#include <errno.h>
#include <glib/gstdio.h>
#include <libfprint/fprint.h>
#include <math.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

#include "fpi-print.h"

/* Alignment hint tuning (see fp-align-tool.c for the derivation). */
#define PX_PER_MM 19.0
#define SEARCH_RANGE 33
#define MIN_OVERLAP_FRAC 0.40
#define FPIMG_MAGIC "FPIM"
#define FPIMG_VERSION 1

typedef struct
{
  guint  width;
  guint  height;
  gsize  size;
  guchar *data;
} CaptureImage;

typedef struct
{
  gchar      *employee_id;
  FpPrint    *print;
  GPtrArray  *images; /* CaptureImage* sub-print images, or NULL */
} PrintRecord;

static void capture_image_free (CaptureImage *image);

static void safe_close_device (FpDevice *device);

static void
capture_image_free (CaptureImage *image)
{
  if (!image)
    return;

  g_free (image->data);
  g_free (image);
}

static void
print_record_free (PrintRecord *record)
{
  if (!record)
    return;

  g_clear_pointer (&record->employee_id, g_free);
  g_clear_object (&record->print);
  if (record->images)
    g_ptr_array_unref (record->images);
  g_free (record);
}

static void
print_line (const char *prefix,
            const char *message)
{
  g_print ("%s|%s\n", prefix, message ? message : "");
  fflush (stdout);
}

static gboolean
valid_employee_id (const char *employee_id)
{
  if (!employee_id || !*employee_id)
    return FALSE;

  for (const char *cursor = employee_id; *cursor; cursor++)
    {
      if (g_ascii_isalnum (*cursor) || *cursor == '_' || *cursor == '-' || *cursor == '.')
        continue;
      return FALSE;
    }

  return TRUE;
}

static gchar *
print_path_for_employee (const char *storage_dir,
                         const char *employee_id)
{
  g_autofree gchar *filename = g_strdup_printf ("%s.fpdata", employee_id);
  return g_build_filename (storage_dir, filename, NULL);
}

static gchar *
image_bundle_path_for_employee (const char *storage_dir,
                                const char *employee_id)
{
  g_autofree gchar *filename = g_strdup_printf ("%s.fpimg", employee_id);
  return g_build_filename (storage_dir, filename, NULL);
}

static void
bundle_append_u32 (GByteArray *bundle,
                   guint32     value)
{
  guint8 bytes[4] = {
    (guint8) (value & 0xff),
    (guint8) ((value >> 8) & 0xff),
    (guint8) ((value >> 16) & 0xff),
    (guint8) ((value >> 24) & 0xff),
  };
  g_byte_array_append (bundle, bytes, sizeof bytes);
}

static guint32
read_u32_le (const gchar *contents,
             gsize        offset)
{
  return (guint32) ((guchar *) contents)[offset] |
         ((guint32) ((guchar *) contents)[offset + 1] << 8) |
         ((guint32) ((guchar *) contents)[offset + 2] << 16) |
         ((guint32) ((guchar *) contents)[offset + 3] << 24);
}

/*
 * .fpimg bundle format (little endian):
 *   'FPIM' | u32 version | u32 count |
 *   per image: u32 width | u32 height | u32 nbytes | nbytes raw pixels
 */
static gboolean
save_image_bundle (GPtrArray  *images,
                   const char *path,
                   GError    **error)
{
  GByteArray *bundle = g_byte_array_new ();
  gboolean ok;

  g_byte_array_append (bundle, (const guint8 *) FPIMG_MAGIC, 4);
  bundle_append_u32 (bundle, FPIMG_VERSION);
  bundle_append_u32 (bundle, images->len);

  for (guint i = 0; i < images->len; i++)
    {
      CaptureImage *image = g_ptr_array_index (images, i);
      bundle_append_u32 (bundle, image->width);
      bundle_append_u32 (bundle, image->height);
      bundle_append_u32 (bundle, image->size);
      g_byte_array_append (bundle, image->data, image->size);
    }

  ok = g_file_set_contents (path, (const gchar *) bundle->data, bundle->len, error);
  g_byte_array_unref (bundle);

  if (ok && g_chmod (path, 0600) != 0)
    g_warning ("Could not chmod %s: %s", path, g_strerror (errno));

  return ok;
}

static GPtrArray *
load_image_bundle (const char *path)
{
  GPtrArray *images = g_ptr_array_new_with_free_func ((GDestroyNotify) capture_image_free);
  g_autofree gchar *contents = NULL;
  gsize length = 0;
  guint32 version;
  guint32 count;
  gsize offset = 12;

  if (!g_file_get_contents (path, &contents, &length, NULL))
    return images;
  if (length < 12 || memcmp (contents, FPIMG_MAGIC, 4) != 0)
    return images;

  version = read_u32_le (contents, 4);
  count = read_u32_le (contents, 8);
  if (version != FPIMG_VERSION)
    return images;

  for (guint32 i = 0; i < count; i++)
    {
      guint32 width;
      guint32 height;
      guint32 size;
      CaptureImage *image;

      if (offset + 12 > length)
        break;
      width = read_u32_le (contents, offset);
      height = read_u32_le (contents, offset + 4);
      size = read_u32_le (contents, offset + 8);
      offset += 12;

      if (width == 0 || height == 0 || size == 0 ||
          (guint64) width * height != size || offset + size > length)
        break;

      image = g_new0 (CaptureImage, 1);
      image->width = width;
      image->height = height;
      image->size = size;
      image->data = g_memdup2 (contents + offset, size);
      g_ptr_array_add (images, image);
      offset += size;
    }

  return images;
}

typedef struct
{
  double ncc;
  int dx, dy;
  int overlap;
} AlignResult;

/*
 * NCC between test and (sub shifted by (dx,dy)) over their overlap.
 * sub_sh(x,y) = sub(x-dx, y-dy); valid where 0 <= x-dx < w.
 * (dx,dy) is the finger's displacement relative to the enrolled position.
 */
static double
ncc_at (const guchar *test, const guchar *sub,
        int w, int h, int dx, int dy, int *overlap_out)
{
  int x0 = dx > 0 ? dx : 0;
  int x1 = dx > 0 ? w : w + dx;
  int y0 = dy > 0 ? dy : 0;
  int y1 = dy > 0 ? h : h + dy;

  if (x1 <= x0 || y1 <= y0)
    return 0.0;

  int n = (x1 - x0) * (y1 - y0);
  double sx = 0, sy = 0, sxx = 0, syy = 0, sxy = 0;

  for (int y = y0; y < y1; y++)
    for (int x = x0; x < x1; x++)
      {
        double a = test[y * w + x];
        double b = sub[(y - dy) * w + (x - dx)];
        sx += a;
        sy += b;
        sxx += a * a;
        syy += b * b;
        sxy += a * b;
      }

  double cov = sxy - sx * sy / n;
  double va = sxx - sx * sx / n;
  double vb = syy - sy * sy / n;
  if (va <= 0 || vb <= 0)
    return 0.0;

  if (overlap_out)
    *overlap_out = n;
  return cov / sqrt (va * vb);
}

static AlignResult
ncc_align (const guchar *test, const guchar *sub,
           int w, int h, int range)
{
  AlignResult best = { -2.0, 0, 0, 0 };
  int min_overlap = (int) (MIN_OVERLAP_FRAC * w * h);

  for (int dy = -range; dy <= range; dy++)
    for (int dx = -range; dx <= range; dx++)
      {
        int overlap = 0;
        double ncc = ncc_at (test, sub, w, h, dx, dy, &overlap);
        if (overlap < min_overlap)
          continue;
        if (ncc > best.ncc)
          {
            best.ncc = ncc;
            best.dx = dx;
            best.dy = dy;
            best.overlap = overlap;
          }
      }

  if (best.ncc < 0)
    best.ncc = 0;
  return best;
}

/*
 * On NO_MATCH, report which enrolled employee came closest and (when that
 * employee has sub-print images) where the scanned finger sits relative to
 * the enrolled position:
 *   BEST|<employee_id>|<score>
 *   HINT|<employee_id>|<ncc_pct>|<slide_x_mm>|<slide_y_mm>
 * Slide values are signed: +x = slide right, +y = slide down.
 */
static void
report_no_match_hint (GPtrArray *records,
                      FpPrint   *scanned_print)
{
  int best_score = -1;
  PrintRecord *best_record = NULL;

  if (!scanned_print)
    return;

  for (guint i = 0; i < records->len; i++)
    {
      PrintRecord *record = g_ptr_array_index (records, i);
      int score = fpi_print_sigfm_best_score (record->print, scanned_print, NULL);
      if (score > best_score)
        {
          best_score = score;
          best_record = record;
        }
    }

  if (!best_record || best_score < 0)
    return;

  g_print ("BEST|%s|%d\n", best_record->employee_id, best_score);
  fflush (stdout);

  FpImage *scanned_image = fp_print_get_image (scanned_print);
  if (!scanned_image || !best_record->images || !best_record->images->len)
    return;

  gsize size = 0;
  const guchar *data = fp_image_get_data (scanned_image, &size);
  int w = fp_image_get_width (scanned_image);
  int h = fp_image_get_height (scanned_image);
  if (!data || size != (gsize) w * h)
    return;

  AlignResult best = { -2.0, 0, 0, 0 };
  for (guint i = 0; i < best_record->images->len; i++)
    {
      CaptureImage *image = g_ptr_array_index (best_record->images, i);
      if ((int) image->width != w || (int) image->height != h)
        continue;
      AlignResult result = ncc_align (data, image->data, w, h, SEARCH_RANGE);
      if (result.ncc > best.ncc)
        best = result;
    }

  if (best.ncc < 0)
    return;

  int pct = (int) (best.ncc * 100.0);
  double slide_x = -best.dx / PX_PER_MM;
  double slide_y = -best.dy / PX_PER_MM;
  g_print ("HINT|%s|%d|%.1f|%.1f\n",
           best_record->employee_id, pct, slide_x, slide_y);
  fflush (stdout);
}

static FpFinger
parse_finger (const char *finger_name)
{
  if (g_strcmp0 (finger_name, "left-thumb") == 0)
    return FP_FINGER_LEFT_THUMB;
  if (g_strcmp0 (finger_name, "left-index") == 0)
    return FP_FINGER_LEFT_INDEX;
  if (g_strcmp0 (finger_name, "left-middle") == 0)
    return FP_FINGER_LEFT_MIDDLE;
  if (g_strcmp0 (finger_name, "left-ring") == 0)
    return FP_FINGER_LEFT_RING;
  if (g_strcmp0 (finger_name, "left-little") == 0)
    return FP_FINGER_LEFT_LITTLE;
  if (g_strcmp0 (finger_name, "right-thumb") == 0)
    return FP_FINGER_RIGHT_THUMB;
  if (g_strcmp0 (finger_name, "right-index") == 0)
    return FP_FINGER_RIGHT_INDEX;
  if (g_strcmp0 (finger_name, "right-middle") == 0)
    return FP_FINGER_RIGHT_MIDDLE;
  if (g_strcmp0 (finger_name, "right-ring") == 0)
    return FP_FINGER_RIGHT_RING;
  if (g_strcmp0 (finger_name, "right-little") == 0)
    return FP_FINGER_RIGHT_LITTLE;

  return FP_FINGER_UNKNOWN;
}

static FpDevice *
find_device (FpContext *context)
{
  GPtrArray *devices = fp_context_get_devices (context);
  FpDevice *fallback = NULL;

  if (!devices || !devices->len)
    return NULL;

  for (guint i = 0; i < devices->len; i++)
    {
      FpDevice *device = g_ptr_array_index (devices, i);

      if (!fallback)
        fallback = device;

      if (g_strcmp0 (fp_device_get_driver (device), "cs9711") == 0)
        return device;
    }

  return fallback;
}

static void
report_device (FpDevice *device)
{
  g_print ("DEVICE|%s|%s|%s\n",
           fp_device_get_name (device),
           fp_device_get_driver (device),
           fp_device_get_device_id (device));
  fflush (stdout);
}

static void
set_enroll_date (FpPrint *print)
{
  g_autoptr(GDateTime) now = g_date_time_new_now_local ();
  g_autoptr(GDate) date = NULL;
  gint year;
  gint month;
  gint day;

  g_date_time_get_ymd (now, &year, &month, &day);
  date = g_date_new_dmy (day, month, year);
  fp_print_set_enroll_date (print, date);
}

static void
enroll_progress_cb (FpDevice *device,
                    gint      completed_stages,
                    FpPrint  *print,
                    gpointer  user_data,
                    GError   *error)
{
  if (error)
    {
      print_line ("RETRY", error->message);
      return;
    }

  g_print ("PROGRESS|%d|%d\n", completed_stages, fp_device_get_nr_enroll_stages (device));
  fflush (stdout);

  /* user_data: GPtrArray of CaptureImage* collecting each sub-print's raw
   * image for the alignment-hint bundle (.fpimg). */
  GPtrArray *images = user_data;
  FpImage *image = print ? fp_print_get_image (print) : NULL;
  if (images && image)
    {
      gsize size = 0;
      const guchar *data = fp_image_get_data (image, &size);
      guint width = fp_image_get_width (image);
      guint height = fp_image_get_height (image);

      if (data && size == (gsize) width * height)
        {
          CaptureImage *capture = g_new0 (CaptureImage, 1);
          capture->width = width;
          capture->height = height;
          capture->size = size;
          capture->data = g_memdup2 (data, size);
          g_ptr_array_add (images, capture);
        }
    }
}

static gboolean
save_print (FpPrint    *print,
            const char *path,
            GError    **error)
{
  g_autofree guchar *data = NULL;
  gsize size = 0;

  if (!fp_print_serialize (print, &data, &size, error))
    return FALSE;

  if (!g_file_set_contents (path, (const gchar *) data, size, error))
    return FALSE;

  if (g_chmod (path, 0600) != 0)
    g_warning ("Could not chmod %s: %s", path, g_strerror (errno));

  return TRUE;
}

static int
enroll_employee (const char *storage_dir,
                 const char *employee_id,
                 const char *finger_name)
{
  g_autoptr(FpContext) context = NULL;
  g_autoptr(GError) error = NULL;
  g_autoptr(FpPrint) enrolled_print = NULL;
  g_autoptr(GPtrArray) images = NULL;
  g_autofree gchar *path = NULL;
  g_autofree gchar *bundle_path = NULL;
  FpDevice *device;
  FpPrint *template_print;
  FpFinger finger;

  if (!valid_employee_id (employee_id))
    {
      print_line ("ERROR", "Invalid employee ID.");
      return 1;
    }

  finger = parse_finger (finger_name);
  if (finger == FP_FINGER_UNKNOWN)
    {
      print_line ("ERROR", "Unknown finger name.");
      return 1;
    }

  if (g_mkdir_with_parents (storage_dir, 0700) != 0)
    {
      print_line ("ERROR", g_strerror (errno));
      return 1;
    }

  context = fp_context_new ();
  device = find_device (context);
  if (!device)
    {
      print_line ("ERROR", "No fingerprint reader detected.");
      return 1;
    }

  report_device (device);

  if (!fp_device_open_sync (device, NULL, &error))
    {
      print_line ("ERROR", error->message);
      return 1;
    }

  g_print ("ENROLL_STAGES|%d\n", fp_device_get_nr_enroll_stages (device));
  print_line ("READY", "enroll");

  template_print = fp_print_new (device);
  fp_print_set_finger (template_print, finger);
  fp_print_set_username (template_print, employee_id);
  fp_print_set_description (template_print, employee_id);
  set_enroll_date (template_print);

  images = g_ptr_array_new_with_free_func ((GDestroyNotify) capture_image_free);
  enrolled_print = fp_device_enroll_sync (device, template_print, NULL, enroll_progress_cb, images, &error);
  if (!enrolled_print)
    {
      print_line ("ERROR", error ? error->message : "Enrollment failed.");
      safe_close_device (device);
      return 1;
    }

  fp_print_set_username (enrolled_print, employee_id);
  fp_print_set_description (enrolled_print, employee_id);
  set_enroll_date (enrolled_print);

  path = print_path_for_employee (storage_dir, employee_id);
  if (!save_print (enrolled_print, path, &error))
    {
      print_line ("ERROR", error->message);
      safe_close_device (device);
      return 1;
    }

  if (images->len)
    {
      bundle_path = image_bundle_path_for_employee (storage_dir, employee_id);
      if (!save_image_bundle (images, bundle_path, &error))
        g_warning ("Could not save image bundle %s: %s", bundle_path, error->message);
    }

  safe_close_device (device);
  g_print ("ENROLLED|%s|%s\n", employee_id, path);
  fflush (stdout);
  return 0;
}

static gboolean
load_print_file (const char *storage_dir,
                 const char *filename,
                 FpDevice   *device,
                 GPtrArray  *records)
{
  g_autofree gchar *path = NULL;
  g_autofree gchar *contents = NULL;
  g_autofree gchar *employee_id = NULL;
  g_autoptr(GError) error = NULL;
  gsize length = 0;
  FpPrint *print;
  PrintRecord *record;

  if (!g_str_has_suffix (filename, ".fpdata"))
    return FALSE;

  employee_id = g_strndup (filename, strlen (filename) - strlen (".fpdata"));
  if (!valid_employee_id (employee_id))
    return FALSE;

  path = g_build_filename (storage_dir, filename, NULL);
  if (!g_file_get_contents (path, &contents, &length, &error))
    {
      g_warning ("Could not read %s: %s", path, error->message);
      return FALSE;
    }

  print = fp_print_deserialize ((const guchar *) contents, length, &error);
  if (!print)
    {
      g_warning ("Could not deserialize %s: %s", path, error->message);
      return FALSE;
    }

  if (!fp_print_compatible (print, device))
    {
      g_object_unref (print);
      return FALSE;
    }

  fp_print_set_username (print, employee_id);
  fp_print_set_description (print, employee_id);

  record = g_new0 (PrintRecord, 1);
  record->employee_id = g_steal_pointer (&employee_id);
  record->print = print;

  g_autofree gchar *bundle_path = image_bundle_path_for_employee (storage_dir, record->employee_id);
  record->images = load_image_bundle (bundle_path);

  g_ptr_array_add (records, record);
  return TRUE;
}

static GPtrArray *
load_records (const char *storage_dir,
              FpDevice   *device)
{
  g_autoptr(GError) error = NULL;
  GPtrArray *records;
  GDir *dir;
  const gchar *filename;

  records = g_ptr_array_new_with_free_func ((GDestroyNotify) print_record_free);
  dir = g_dir_open (storage_dir, 0, &error);
  if (!dir)
    return records;

  while ((filename = g_dir_read_name (dir)) != NULL)
    load_print_file (storage_dir, filename, device, records);

  g_dir_close (dir);
  return records;
}

static PrintRecord *
find_record_for_match (GPtrArray *records,
                       FpPrint   *match)
{
  const char *match_id;

  if (!match)
    return NULL;

  match_id = fp_print_get_description (match);
  if (!match_id || !*match_id)
    match_id = fp_print_get_username (match);

  for (guint i = 0; i < records->len; i++)
    {
      PrintRecord *record = g_ptr_array_index (records, i);

      if (record->print == match)
        return record;

      if (match_id && g_strcmp0 (record->employee_id, match_id) == 0)
        return record;
    }

  return NULL;
}

static void
safe_close_device (FpDevice *device)
{
  g_autoptr(GError) close_error = NULL;

  fp_device_close_sync (device, NULL, &close_error);
  if (close_error)
    {
      g_warning ("Close attempt 1 warning: %s", close_error->message);
      close_error = NULL;
      g_usleep (200 * 1000);
      fp_device_close_sync (device, NULL, &close_error);
    }

  /* CS9711 dev_close releases the USB interface but pending async transfers may
   * still be draining in the kernel. A brief pause ensures the next process can
   * claim the interface without hitting EBUSY. */
  g_usleep (300 * 1000);
}

static int
identify_employee (const char *storage_dir)
{
  g_autoptr(FpContext) context = NULL;
  g_autoptr(GPtrArray) records = NULL;
  g_autoptr(GPtrArray) gallery = NULL;
  g_autoptr(GError) error = NULL;
  g_autoptr(FpPrint) match = NULL;
  g_autoptr(FpPrint) scanned_print = NULL;
  FpDevice *device;
  gboolean ok = FALSE;
  int ret = 0;

  context = fp_context_new ();
  device = find_device (context);
  if (!device)
    {
      print_line ("ERROR", "No fingerprint reader detected.");
      return 1;
    }

  report_device (device);

  if (!fp_device_open_sync (device, NULL, &error))
    {
      print_line ("ERROR", error->message);
      return 1;
    }

  records = load_records (storage_dir, device);
  if (!records->len)
    {
      print_line ("ERROR", "No compatible enrolled fingerprints found.");
      safe_close_device (device);
      return 1;
    }

  gallery = g_ptr_array_new ();
  for (guint i = 0; i < records->len; i++)
    {
      PrintRecord *record = g_ptr_array_index (records, i);
      g_ptr_array_add (gallery, record->print);
    }

  for (guint attempt = 0; attempt < 3; attempt++)
    {
      g_clear_error (&error);
      g_clear_object (&match);
      g_clear_object (&scanned_print);

      g_print ("ATTEMPT|%u|3|waiting\n", attempt + 1);
      fflush (stdout);
      ok = fp_device_identify_sync (device, gallery, NULL, NULL, NULL, &match, &scanned_print, &error);
      if (ok)
        break;

      if (error && error->domain == FP_DEVICE_RETRY)
        {
          print_line ("RETRY", error->message);
          g_clear_error (&error);
          g_usleep (500 * 1000); /* 500ms pause before next scan attempt */
          continue;
        }

      print_line ("ERROR", error ? error->message : "Identification failed.");
      safe_close_device (device);
      return 1;
    }

  if (!ok)
    {
      print_line ("ERROR", error ? error->message : "Identification failed.");
      safe_close_device (device);
      g_usleep (1 * 1000 * 1000); /* let USB driver fully release */
      return 1;
    }

  if (match)
    {
      PrintRecord *record = find_record_for_match (records, match);
      if (record)
        g_print ("MATCH|%s\n", record->employee_id);
      else
        {
          print_line ("ERROR", "Matched print was not found in local registry.");
          ret = 1;
        }
    }
  else
    {
      report_no_match_hint (records, scanned_print);
      g_print ("NO_MATCH\n");
    }

  fflush (stdout);
  safe_close_device (device);
  return ret;
}

static void
usage (const char *program)
{
  g_printerr ("Usage:\n");
  g_printerr ("  %s enroll <storage-dir> <employee-id> <finger>\n", program);
  g_printerr ("  %s identify <storage-dir>\n", program);
}

int
main (int argc, char **argv)
{
  setenv ("G_MESSAGES_DEBUG", "all", 0);

  if (argc >= 2 && g_strcmp0 (argv[1], "enroll") == 0)
    {
      if (argc != 5)
        {
          usage (argv[0]);
          return 1;
        }
      return enroll_employee (argv[2], argv[3], argv[4]);
    }

  if (argc >= 2 && g_strcmp0 (argv[1], "identify") == 0)
    {
      if (argc != 3)
        {
          usage (argv[0]);
          return 1;
        }
      return identify_employee (argv[2]);
    }

  usage (argv[0]);
  return 1;
}
