/*
 * fp-align-tool: interactive alignment diagnostic for the CS9711.
 *
 * Usage:
 *   fp-align-tool enroll <dir> <employee-id> [n=15]
 *   fp-align-tool scan <dir> [employee-id]
 *   fp-align-tool align <dir> <test.pgm> [employee-id]
 *
 * Layout under <dir>:
 *   <employee-id>/<employee-id>.fpdata   production template (SIGFM sub-prints)
 *   <employee-id>/subprint-NN.pgm        68x118 sub-print images (NCC alignment)
 *
 * enroll: guided capture (default 15 sub-prints: 5 center / 3 lower / 3 higher /
 *         2 left / 2 right), saves the PGM images and a production-format
 *         .fpdata (same serialization the helper uses).
 * scan:   live loop: place finger -> capture 5 frames -> per employee: NCC
 *         offset alignment (match % + which way to slide) and the production
 *         SIFT score -> ENTER to rescan, q to quit.
 * align:  same analysis on a saved PGM (offline, no sensor needed).
 *
 * Alignment model: the test image is the enrolled sub-print shifted by the
 * finger's displacement (dx,dy). A 2D search over dx,dy in [-25,25] maximizes
 * normalized cross-correlation over the overlap. The best offset is where the
 * finger currently sits; sliding by (-dx,-dy) recenters it.
 */

#define FP_COMPONENT "fp-align-tool"

#include <errno.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <libfprint/fprint.h>
#include "fpi-print.h"
#include "fp-print-private.h"
#include "sigfm/sigfm.h"

#define IMAGE_WIDTH 68
#define IMAGE_HEIGHT 118
#define PX_PER_MM 19.0
#define SEARCH_RANGE 33
#define SIFT_THRESHOLD 40
#define FRAMES_PER_SCAN 5
#define MIN_OVERLAP_FRAC 0.40
#define MAX_EMPLOYEES 16
#define MAX_SUBPRINTS 64

typedef struct
{
  double ncc;
  int dx, dy;
  int overlap;
} AlignResult;

typedef struct
{
  char *id;
  GPtrArray *subs;    /* SigfmImgInfo* (copies) from .fpdata, or NULL */
  GPtrArray *images;  /* unsigned char* PGM images, or NULL */
} Employee;

typedef struct
{
  double ncc;         /* best NCC for this employee, -2 if not computed */
  int dx, dy;
  int overlap;
  int sub_idx;
  int sift_max;       /* -1 if not computed */
  int sift_idx;
} EmpResult;

static gboolean
write_pgm (const char *path, const unsigned char *data, int width, int height)
{
  FILE *fd = fopen (path, "wb");
  size_t want = (size_t) width * height;

  if (!fd)
    {
      g_printerr ("ERROR|could not open '%s': %s\n", path, g_strerror (errno));
      return FALSE;
    }

  fprintf (fd, "P5\n%d %d\n255\n", width, height);
  if (fwrite (data, 1, want, fd) != want)
    {
      fclose (fd);
      g_printerr ("ERROR|short write to '%s'\n", path);
      return FALSE;
    }

  fclose (fd);
  return TRUE;
}

static void
pgm_skip_ws (FILE *fd)
{
  int c;
  while ((c = fgetc (fd)) != EOF)
    {
      if (c == '#')
        {
          while ((c = fgetc (fd)) != EOF && c != '\n')
            ;
        }
      else if (c > ' ')
        {
          ungetc (c, fd);
          return;
        }
    }
}

static gboolean
read_pgm (const char *path, unsigned char **out, int *width, int *height)
{
  FILE *fd = fopen (path, "rb");

  *out = NULL;
  if (!fd)
    return FALSE;

  char magic[3] = {0};
  if (fscanf (fd, "%2s", magic) != 1 || strcmp (magic, "P5") != 0)
    {
      fclose (fd);
      return FALSE;
    }
  pgm_skip_ws (fd);
  if (fscanf (fd, "%d", width) != 1)
    {
      fclose (fd);
      return FALSE;
    }
  pgm_skip_ws (fd);
  if (fscanf (fd, "%d", height) != 1)
    {
      fclose (fd);
      return FALSE;
    }
  pgm_skip_ws (fd);
  int maxval;
  if (fscanf (fd, "%d", &maxval) != 1 || maxval != 255)
    {
      fclose (fd);
      return FALSE;
    }
  fgetc (fd);

  *out = (unsigned char *) g_malloc ((size_t) *width * *height);
  if (fread (*out, 1, (size_t) *width * *height, fd) != (size_t) *width * *height)
    {
      g_free (*out);
      *out = NULL;
      fclose (fd);
      return FALSE;
    }

  fclose (fd);
  return TRUE;
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

/*
 * NCC between test and (sub shifted by (dx,dy)) over their overlap.
 * sub_sh(x,y) = sub(x-dx, y-dy); valid where 0 <= x-dx < w.
 */
static double
ncc_at (const unsigned char *test, const unsigned char *sub,
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
ncc_align (const unsigned char *test, const unsigned char *sub,
           int w, int h, int range)
{
  AlignResult best = {-2.0, 0, 0, 0};
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

static int
sift_best (const unsigned char *pix, int w, int h, GPtrArray *subs, int *best_idx)
{
  SigfmImgInfo *info = sigfm_extract (pix, w, h);
  if (!info || !subs || !subs->len)
    {
      if (info)
        sigfm_free_info (info);
      if (best_idx)
        *best_idx = -1;
      return -1;
    }

  int best = -1;
  int best_i = -1;
  for (guint i = 0; i < subs->len; i++)
    {
      int s = sigfm_match_score (info, g_ptr_array_index (subs, i));
      if (s > best)
        {
          best = s;
          best_i = (int) i;
        }
    }

  if (best_idx)
    *best_idx = best_i;
  sigfm_free_info (info);
  return best;
}

static void
employee_free (Employee *e)
{
  if (!e)
    return;
  g_free (e->id);
  if (e->subs)
    g_ptr_array_unref (e->subs);
  if (e->images)
    g_ptr_array_unref (e->images);
  g_free (e);
}

static Employee *
load_employee (const char *dir, const char *id)
{
  Employee *e = (Employee *) g_new0 (Employee, 1);
  e->id = g_strdup (id);

  /* .fpdata -> copy the SigfmImgInfo sub-prints (the FpPrint owns its own) */
  gchar *fp_name = g_strdup_printf ("%s.fpdata", id);
  gchar *fp_path = g_build_filename (dir, id, fp_name, NULL);
  g_free (fp_name);
  gchar *contents = NULL;
  gsize len = 0;
  GError *err = NULL;
  if (g_file_get_contents (fp_path, &contents, &len, &err))
    {
      FpPrint *p = fp_print_deserialize ((const guchar *) contents, len, &err);
      if (p && p->prints)
        {
          e->subs = g_ptr_array_new_with_free_func (
            (GDestroyNotify) sigfm_free_info);
          for (guint i = 0; i < p->prints->len; i++)
            g_ptr_array_add (e->subs,
                             sigfm_copy_info (g_ptr_array_index (p->prints, i)));
        }
      g_clear_error (&err);
      if (p)
        g_object_unref (p);
    }
  g_free (contents);
  g_free (fp_path);

  /* subprint PGMs */
  e->images = g_ptr_array_new_with_free_func (g_free);
  for (int i = 1; i <= MAX_SUBPRINTS; i++)
    {
      gchar *path = g_strdup_printf ("%s/%s/subprint-%02d.pgm", dir, id, i);
      unsigned char *img = NULL;
      int w = 0, h = 0;
      if (read_pgm (path, &img, &w, &h) && w == IMAGE_WIDTH && h == IMAGE_HEIGHT)
        g_ptr_array_add (e->images, img);
      else
        g_free (img);
      g_free (path);
    }

  if (!e->subs && !e->images->len)
    {
      employee_free (e);
      return NULL;
    }
  return e;
}

static GPtrArray *
load_employees (const char *dir, const char *only_id)
{
  GPtrArray *out = g_ptr_array_new_with_free_func ((GDestroyNotify) employee_free);
  GDir *d = g_dir_open (dir, 0, NULL);
  if (!d)
    return out;

  const char *name;
  while ((name = g_dir_read_name (d)) != NULL)
    {
      if (only_id && g_strcmp0 (name, only_id) != 0)
        continue;
      Employee *e = load_employee (dir, name);
      if (e)
        g_ptr_array_add (out, e);
    }
  g_dir_close (d);
  return out;
}

/* Per-employee analysis of one frame. */
static void
analyze_frame (const unsigned char *pix, GPtrArray *employees, EmpResult *res)
{
  for (guint ei = 0; ei < employees->len; ei++)
    {
      Employee *e = g_ptr_array_index (employees, ei);
      EmpResult *r = &res[ei];
      r->ncc = -2.0;
      r->sift_max = -1;
      r->sift_idx = -1;
      r->sub_idx = -1;

      if (e->images)
        for (guint si = 0; si < e->images->len; si++)
          {
            unsigned char *sub = g_ptr_array_index (e->images, si);
            AlignResult a = ncc_align (pix, sub, IMAGE_WIDTH, IMAGE_HEIGHT,
                                       SEARCH_RANGE);
            if (a.ncc > r->ncc)
              {
                r->ncc = a.ncc;
                r->dx = a.dx;
                r->dy = a.dy;
                r->overlap = a.overlap;
                r->sub_idx = (int) si;
              }
          }

      if (e->subs)
        {
          int idx = -1;
          int s = sift_best (pix, IMAGE_WIDTH, IMAGE_HEIGHT, e->subs, &idx);
          if (s > r->sift_max)
            {
              r->sift_max = s;
              r->sift_idx = idx;
            }
        }
    }
}

static const char *
axis_word (int v, const char *pos, const char *neg)
{
  if (v > 2)
    return pos;
  if (v < -2)
    return neg;
  return "centered";
}

static void
print_direction (int dx, int dy)
{
  /* (dx,dy) = finger displacement vs the enrolled position; slide by (-dx,-dy). */
  double slide_x = -dx / PX_PER_MM;
  double slide_y = -dy / PX_PER_MM;

  printf ("         finger %s %dpx (%.1fmm) / %s %dpx (%.1fmm)\n",
          axis_word (dx, "RIGHT", "LEFT"), abs (dx), fabs (dx) / PX_PER_MM,
          axis_word (dy, "DOWN", "UP"), abs (dy), fabs (dy) / PX_PER_MM);

  if (abs (dx) <= 2 && abs (dy) <= 2)
    printf ("         ~ centered — press again\n");
  else
    {
      printf ("         -> slide ");
      if (abs (dx) > 2)
        printf ("%s %.1fmm", slide_x > 0 ? "RIGHT" : "LEFT", fabs (slide_x));
      else
        printf ("hold X");
      if (abs (dy) > 2)
        printf (", %s %.1fmm", slide_y > 0 ? "DOWN" : "UP", fabs (slide_y));
      printf ("\n");
    }
}

static void
print_heatmap (const unsigned char *pix, const unsigned char *sub, int w, int h)
{
  /* NCC % over a fixed grid centered on the aligned position (0,0). */
  printf ("  heat  : NCC%% by offset (step 5px), peak = where the finger sits\n");
  printf ("          dx ");
  for (int dx = -SEARCH_RANGE; dx <= SEARCH_RANGE; dx += 5)
    printf ("%4d", dx);
  printf ("\n");
  for (int dy = -SEARCH_RANGE; dy <= SEARCH_RANGE; dy += 5)
    {
      printf ("        %4d", dy);
      for (int dx = -SEARCH_RANGE; dx <= SEARCH_RANGE; dx += 5)
        {
          int overlap = 0;
          double ncc = ncc_at (pix, sub, w, h, dx, dy, &overlap);
          int pct = (int) (ncc > 0 ? ncc * 100 : 0);
          printf ("%4d", pct);
        }
      printf ("\n");
    }
}

static void
print_employee (const char *id, const EmpResult *r, const Employee *e,
                const unsigned char *pix)
{
  printf ("%s\n", id);

  if (r->ncc >= 0 && e->images && r->sub_idx >= 0)
    {
      int pct = (int) (r->ncc * 100);
      int overlap_pct = (int) (100.0 * r->overlap / (IMAGE_WIDTH * IMAGE_HEIGHT));
      printf ("  align : %d%% match · %d%% overlap · best subprint %d/%d\n",
              pct, overlap_pct, r->sub_idx + 1, (int) e->images->len);
      print_direction (r->dx, r->dy);
      const unsigned char *sub = g_ptr_array_index (e->images, r->sub_idx);
      print_heatmap (pix, sub, IMAGE_WIDTH, IMAGE_HEIGHT);
    }
  else
    printf ("  align : (no subprint PGMs)\n");

  if (r->sift_max >= 0 && e->subs)
    {
      printf ("  sift  : max %d / %d -> %s (subprint %d/%d)\n",
              r->sift_max, SIFT_THRESHOLD,
              r->sift_max >= SIFT_THRESHOLD ? "MATCH" : "NO MATCH",
              r->sift_idx + 1, (int) e->subs->len);
    }
  else
    printf ("  sift  : (no .fpdata)\n");

  printf ("\n");
}

static void
print_results (int scan_no, int nframes, GPtrArray *employees,
               const EmpResult *res, const unsigned char *pix)
{
  printf ("\n=== scan %d (best of %d frames) ===\n\n", scan_no, nframes);

  /* best employee first */
  int best_e = -1;
  double best_ncc = -2;
  for (guint ei = 0; ei < employees->len; ei++)
    {
      if (res[ei].ncc > best_ncc)
        {
          best_ncc = res[ei].ncc;
          best_e = (int) ei;
        }
    }

  if (best_e >= 0)
    {
      Employee *e = g_ptr_array_index (employees, best_e);
      print_employee (e->id, &res[best_e], e, pix);
    }
  for (guint ei = 0; ei < employees->len; ei++)
    {
      Employee *e;
      if ((int) ei == best_e)
        continue;
      e = g_ptr_array_index (employees, ei);
      print_employee (e->id, &res[ei], e, pix);
    }
}

static gboolean
wait_line (char *line, size_t len)
{
  if (!fgets (line, (int) len, stdin))
    return FALSE;
  return TRUE;
}

static int
cmd_enroll (const char *dir, const char *id, int n)
{
  g_autoptr(FpContext) context = NULL;
  g_autoptr(GError) error = NULL;
  g_autoptr(GError) close_error = NULL;
  g_autoptr(FpPrint) print = NULL;
  FpDevice *device;

  if (n < 1 || n > MAX_SUBPRINTS)
    n = 15;

  context = fp_context_new ();
  device = find_device (context);
  if (!device)
    {
      g_printerr ("ERROR|No fingerprint reader detected.\n");
      return 1;
    }

  if (!fp_device_open_sync (device, NULL, &error))
    {
      g_printerr ("ERROR|%s\n", error->message);
      return 1;
    }

  if (g_mkdir_with_parents (g_build_filename (dir, id, NULL), 0700) != 0)
    {
      g_printerr ("ERROR|%s\n", g_strerror (errno));
      return 1;
    }

  print = fp_print_new (device);
  fpi_print_set_type (print, FPI_PRINT_SIGFM);
  fp_print_set_username (print, id);
  fp_print_set_description (print, id);
  fp_print_set_finger (print, FP_FINGER_RIGHT_INDEX);
  {
    time_t now = time (NULL);
    struct tm *lt = localtime (&now);
    fp_print_set_enroll_date (print,
                              g_date_new_dmy (lt->tm_mday, lt->tm_mon + 1,
                                              lt->tm_year + 1900));
  }

  static const char *guided[] = {
    "CENTER", "CENTER", "CENTER", "CENTER", "CENTER",
    "LOWER - move finger down ~1-2mm", "LOWER - move finger down ~1-2mm",
    "LOWER - move finger down ~1-2mm",
    "HIGHER - move finger up ~1-2mm", "HIGHER - move finger up ~1-2mm",
    "HIGHER - move finger up ~1-2mm",
    "LEFT - move finger left ~1-2mm", "LEFT - move finger left ~1-2mm",
    "RIGHT - move finger right ~1-2mm", "RIGHT - move finger right ~1-2mm",
  };

  int done = 0;
  char line[256];
  while (done < n)
    {
      const char *prompt = (n == 15 && done < 15) ? guided[done]
                                                   : "VARY POSITION ~1-2mm";
      printf ("[%d/%d] %s — place finger, press ENTER\n", done + 1, n, prompt);
      fflush (stdout);
      if (!wait_line (line, sizeof line))
        break;

      g_clear_error (&error);
      g_autoptr(FpImage) image = fp_device_capture_sync (device, TRUE, NULL,
                                                         &error);
      if (!image)
        {
          printf ("  capture failed: %s — press ENTER to retry this sub-print\n",
                  error ? error->message : "unknown");
          fflush (stdout);
          if (!wait_line (line, sizeof line))
            break;
          continue;
        }

      gsize size = 0;
      const unsigned char *data = fp_image_get_data (image, &size);
      if (fp_image_get_width (image) != IMAGE_WIDTH ||
          fp_image_get_height (image) != IMAGE_HEIGHT ||
          size != (gsize) IMAGE_WIDTH * IMAGE_HEIGHT)
        {
          printf ("  unexpected image size — retry\n");
          fflush (stdout);
          if (!wait_line (line, sizeof line))
            break;
          continue;
        }

      SigfmImgInfo *info = sigfm_extract (data, IMAGE_WIDTH, IMAGE_HEIGHT);
      if (!info)
        {
          printf ("  no SIFT data — retry\n");
          fflush (stdout);
          if (!wait_line (line, sizeof line))
            break;
          continue;
        }

      gchar *path = g_strdup_printf ("%s/%s/subprint-%02d.pgm", dir, id,
                                     done + 1);
      gboolean ok = write_pgm (path, data, IMAGE_WIDTH, IMAGE_HEIGHT);
      g_free (path);
      if (!ok)
        {
          sigfm_free_info (info);
          printf ("  write failed — retry\n");
          fflush (stdout);
          if (!wait_line (line, sizeof line))
            break;
          continue;
        }

      g_ptr_array_add (print->prints, info);
      done++;
      printf ("  ok\n");
      fflush (stdout);
    }

  if (done < 3)
    {
      g_printerr ("ERROR|too few sub-prints captured (%d)\n", done);
      return 1;
    }

  guchar *serialized = NULL;
  gsize slen = 0;
  GError *serr = NULL;
  if (!fp_print_serialize (print, &serialized, &slen, &serr))
    {
      g_printerr ("ERROR|serialize failed: %s\n",
                  serr ? serr->message : "unknown");
      g_clear_error (&serr);
      return 1;
    }
  gchar *fp_name2 = g_strdup_printf ("%s.fpdata", id);
  gchar *fp_path = g_build_filename (dir, id, fp_name2, NULL);
  g_free (fp_name2);
  if (!g_file_set_contents (fp_path, (const char *) serialized, slen, NULL))
    {
      g_printerr ("ERROR|could not write %s\n", fp_path);
      g_free (serialized);
      g_free (fp_path);
      return 1;
    }
  g_free (serialized);
  g_free (fp_path);

  printf ("enrolled %d sub-prints for %s in %s/%s/\n", done, id, dir, id);
  printf ("(copy %s/%s/%s.fpdata into the production storage dir to use it there)\n",
          dir, id, id);

  fp_device_close_sync (device, NULL, &close_error);
  if (close_error)
    g_warning ("close: %s", close_error->message);
  g_usleep (300 * 1000);
  return 0;
}

static int
run_analysis (GPtrArray *employees, const unsigned char *pix,
              const char *label)
{
  EmpResult res[MAX_EMPLOYEES];
  memset (res, 0, sizeof res);
  analyze_frame (pix, employees, res);

  printf ("\n=== %s ===\n\n", label);
  for (guint ei = 0; ei < employees->len; ei++)
    {
      Employee *e = g_ptr_array_index (employees, ei);
      print_employee (e->id, &res[ei], e, pix);
    }
  return 0;
}

static int
cmd_align (const char *dir, const char *pgm_path, const char *only_id)
{
  unsigned char *pix = NULL;
  int w = 0, h = 0;
  if (!read_pgm (pgm_path, &pix, &w, &h) || w != IMAGE_WIDTH || h != IMAGE_HEIGHT)
    {
      g_printerr ("ERROR|cannot read %s as a %dx%d PGM\n", pgm_path,
                  IMAGE_WIDTH, IMAGE_HEIGHT);
      return 1;
    }

  g_autoptr(GPtrArray) employees = load_employees (dir, only_id);
  if (!employees->len)
    {
      g_printerr ("ERROR|no employees found in %s\n", dir);
      return 1;
    }

  run_analysis (employees, pix, "offline align");
  return 0;
}

static int
cmd_scan (const char *dir, const char *only_id)
{
  g_autoptr(FpContext) context = NULL;
  g_autoptr(GError) error = NULL;
  g_autoptr(GError) close_error = NULL;
  g_autoptr(GPtrArray) employees = load_employees (dir, only_id);
  FpDevice *device;

  if (!employees->len)
    {
      g_printerr ("ERROR|no employees found in %s\n", dir);
      return 1;
    }

  context = fp_context_new ();
  device = find_device (context);
  if (!device)
    {
      g_printerr ("ERROR|No fingerprint reader detected.\n");
      return 1;
    }

  if (!fp_device_open_sync (device, NULL, &error))
    {
      g_printerr ("ERROR|%s\n", error->message);
      return 1;
    }

  int scan_no = 0;
  char line[256];
  for (;;)
    {
      scan_no++;
      unsigned char *frames[FRAMES_PER_SCAN];
      int nframes = 0;

      printf ("place finger (capturing %d frames)...\n", FRAMES_PER_SCAN);
      fflush (stdout);
      for (int i = 0; i < FRAMES_PER_SCAN; i++)
        {
          g_clear_error (&error);
          g_autoptr(FpImage) image =
            fp_device_capture_sync (device, TRUE, NULL, &error);
          if (!image)
            {
              printf ("  frame %d/%d failed: %s\n", i + 1, FRAMES_PER_SCAN,
                      error ? error->message : "unknown");
              fflush (stdout);
              continue;
            }
          gsize size = 0;
          const unsigned char *data = fp_image_get_data (image, &size);
          if (fp_image_get_width (image) != IMAGE_WIDTH ||
              fp_image_get_height (image) != IMAGE_HEIGHT ||
              size != (gsize) IMAGE_WIDTH * IMAGE_HEIGHT)
            {
              printf ("  frame %d/%d: unexpected size, skipped\n", i + 1,
                      FRAMES_PER_SCAN);
              fflush (stdout);
              continue;
            }
          frames[nframes++] = (unsigned char *) g_memdup2 (data, size);
        }

      if (!nframes)
        {
          printf ("no frames — press ENTER to retry, q to quit\n");
          fflush (stdout);
          if (!wait_line (line, sizeof line) || line[0] == 'q' ||
              line[0] == 'Q')
            break;
          continue;
        }

      /* analyze all frames, keep the one with the best NCC */
      const unsigned char *best_pix = frames[0];
      EmpResult best_res[MAX_EMPLOYEES];
      double best_ncc = -2;
      for (int fi = 0; fi < nframes; fi++)
        {
          EmpResult res[MAX_EMPLOYEES];
          memset (res, 0, sizeof res);
          analyze_frame (frames[fi], employees, res);
          double frame_ncc = -2;
          for (guint ei = 0; ei < employees->len; ei++)
            if (res[ei].ncc > frame_ncc)
              frame_ncc = res[ei].ncc;
          if (frame_ncc > best_ncc)
            {
              best_ncc = frame_ncc;
              best_pix = frames[fi];
              memcpy (best_res, res, sizeof res);
            }
        }

      print_results (scan_no, nframes, employees, best_res, best_pix);

      printf ("[ENTER] rescan · [q] quit\n");
      fflush (stdout);
      if (!wait_line (line, sizeof line) || line[0] == 'q' || line[0] == 'Q')
        break;

      for (int fi = 0; fi < nframes; fi++)
        g_free (frames[fi]);
    }

  fp_device_close_sync (device, NULL, &close_error);
  if (close_error)
    g_warning ("close: %s", close_error->message);
  g_usleep (300 * 1000);
  return 0;
}

static void
usage (const char *program)
{
  g_printerr ("Usage:\n");
  g_printerr ("  %s enroll <dir> <employee-id> [n=15]\n", program);
  g_printerr ("  %s scan <dir> [employee-id]\n", program);
  g_printerr ("  %s align <dir> <test.pgm> [employee-id]\n", program);
}

int
main (int argc, char **argv)
{
  if (argc < 2)
    {
      usage (argv[0]);
      return 1;
    }

  if (g_strcmp0 (argv[1], "enroll") == 0)
    {
      if (argc < 4)
        {
          usage (argv[0]);
          return 1;
        }
      int n = (argc > 4) ? atoi (argv[4]) : 15;
      return cmd_enroll (argv[2], argv[3], n);
    }
  if (g_strcmp0 (argv[1], "scan") == 0)
    {
      if (argc < 3)
        {
          usage (argv[0]);
          return 1;
        }
      return cmd_scan (argv[2], argc > 3 ? argv[3] : NULL);
    }
  if (g_strcmp0 (argv[1], "align") == 0)
    {
      if (argc < 4)
        {
          usage (argv[0]);
          return 1;
        }
      return cmd_align (argv[2], argv[3], argc > 4 ? argv[4] : NULL);
    }

  usage (argv[0]);
  return 1;
}
