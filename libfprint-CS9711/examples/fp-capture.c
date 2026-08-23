/*
 * fp-capture: capture N scan frames from the CS9711 for diagnostics.
 *
 * Usage: fp-capture <dir> [n]
 *
 * For each frame (one full scan cycle per frame) saves:
 *   frame-NN.pgm      the decoded 68x118 image (what the matcher sees)
 *   frame-NN-raw.pgm  the reconstructed 34x236 sensor stream layout
 *
 * The raw layout is invertible from the decoded image: the driver stores
 * img[y/2][2x + y%2] = raw[y][x], so raw[y][x] = img[y/2][2x + y%2].
 */

#define FP_COMPONENT "fp-capture"

#include <errno.h>
#include <libfprint/fprint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define SENSOR_WIDTH 34
#define SENSOR_HEIGHT 236
#define IMAGE_WIDTH 68
#define IMAGE_HEIGHT 118

static gboolean
write_pgm (const char *path, const unsigned char *data, int width, int height)
{
  FILE *fd = fopen (path, "wb");
  size_t want = (size_t) width * height;

  if (!fd)
    {
      g_warning ("could not open '%s': %s", path, g_strerror (errno));
      return FALSE;
    }

  fprintf (fd, "P5\n%d %d\n255\n", width, height);
  if (fwrite (data, 1, want, fd) != want)
    {
      fclose (fd);
      g_warning ("short write to '%s'", path);
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

int
main (int argc, char **argv)
{
  const char *dir = (argc > 1) ? argv[1] : ".";
  int n = (argc > 2) ? atoi (argv[2]) : 10;
  g_autoptr(FpContext) context = NULL;
  g_autoptr(GError) error = NULL;
  g_autoptr(GError) close_error = NULL;
  g_autofree unsigned char *raw = NULL;
  FpDevice *device;

  setenv ("G_MESSAGES_DEBUG", "all", 0);

  if (n < 1)
    n = 1;

  if (g_mkdir_with_parents (dir, 0700) != 0)
    {
      g_printerr ("ERROR|%s\n", g_strerror (errno));
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

  raw = g_malloc0 (SENSOR_WIDTH * SENSOR_HEIGHT);

  for (int i = 0; i < n; i++)
    {
      g_autoptr(FpImage) image = NULL;
      g_autofree gchar *path = NULL;
      g_autofree gchar *raw_path = NULL;
      char line[256];
      gsize size = 0;
      const unsigned char *data;

      g_print ("FRAME|%d|%d|place finger, press ENTER\n", i + 1, n);
      fflush (stdout);

      if (!fgets (line, 256, stdin))
        {
          g_print ("ERROR|stdin closed\n");
          break;
        }

      image = fp_device_capture_sync (device, TRUE, NULL, &error);
      if (!image)
        {
          g_printerr ("ERROR|%s\n", error ? error->message : "capture failed");
          error = NULL;
          continue;
        }

      data = fp_image_get_data (image, &size);
      if (fp_image_get_width (image) != IMAGE_WIDTH ||
          fp_image_get_height (image) != IMAGE_HEIGHT ||
          size != (gsize) IMAGE_WIDTH * IMAGE_HEIGHT)
        {
          g_printerr ("ERROR|unexpected image %dx%d (%lu bytes)\n",
                      fp_image_get_width (image), fp_image_get_height (image),
                      (unsigned long) size);
          continue;
        }

      for (int y = 0; y < SENSOR_HEIGHT; y++)
        for (int x = 0; x < SENSOR_WIDTH; x++)
          raw[y * SENSOR_WIDTH + x] = data[(y / 2) * IMAGE_WIDTH + (x * 2 + y % 2)];

      path = g_strdup_printf ("%s/frame-%02d.pgm", dir, i + 1);
      raw_path = g_strdup_printf ("%s/frame-%02d-raw.pgm", dir, i + 1);

      if (write_pgm (path, data, IMAGE_WIDTH, IMAGE_HEIGHT) &&
          write_pgm (raw_path, raw, SENSOR_WIDTH, SENSOR_HEIGHT))
        g_print ("CAPTURED|%d|%s\n", i + 1, path);
      else
        g_print ("ERROR|write failed\n");
      fflush (stdout);
    }

  fp_device_close_sync (device, NULL, &close_error);
  if (close_error)
    g_warning ("close: %s", close_error->message);
  g_usleep (300 * 1000);

  return 0;
}
