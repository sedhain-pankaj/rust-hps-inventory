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
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

typedef struct
{
  gchar   *employee_id;
  FpPrint *print;
} PrintRecord;

static void
print_record_free (PrintRecord *record)
{
  if (!record)
    return;

  g_clear_pointer (&record->employee_id, g_free);
  g_clear_object (&record->print);
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
  g_autofree gchar *path = NULL;
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

  enrolled_print = fp_device_enroll_sync (device, template_print, NULL, enroll_progress_cb, NULL, &error);
  if (!enrolled_print)
    {
      print_line ("ERROR", error ? error->message : "Enrollment failed.");
      fp_device_close_sync (device, NULL, NULL);
      return 1;
    }

  fp_print_set_username (enrolled_print, employee_id);
  fp_print_set_description (enrolled_print, employee_id);
  set_enroll_date (enrolled_print);

  path = print_path_for_employee (storage_dir, employee_id);
  if (!save_print (enrolled_print, path, &error))
    {
      print_line ("ERROR", error->message);
      fp_device_close_sync (device, NULL, NULL);
      return 1;
    }

  fp_device_close_sync (device, NULL, NULL);
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
      fp_device_close_sync (device, NULL, NULL);
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

      print_line ("READY", "identify");
      ok = fp_device_identify_sync (device, gallery, NULL, NULL, NULL, &match, &scanned_print, &error);
      if (ok)
        break;

      if (error && error->domain == FP_DEVICE_RETRY)
        {
          print_line ("RETRY", error->message);
          continue;
        }

      print_line ("ERROR", error ? error->message : "Identification failed.");
      fp_device_close_sync (device, NULL, NULL);
      return 1;
    }

  if (!ok)
    {
      print_line ("ERROR", error ? error->message : "Identification failed.");
      fp_device_close_sync (device, NULL, NULL);
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
      g_print ("NO_MATCH\n");
    }

  fflush (stdout);
  fp_device_close_sync (device, NULL, NULL);
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
