#ifndef GDAY_OPUS_FILE_BRIDGE_H
#define GDAY_OPUS_FILE_BRIDGE_H
#include <stdint.h>
#include <stddef.h>

typedef struct GdayOpusFile GdayOpusFile;
GdayOpusFile *gday_opus_open(const char *path, int *error);
void gday_opus_close(GdayOpusFile *file);
int gday_opus_channels(GdayOpusFile *file);
int64_t gday_opus_frames(GdayOpusFile *file);
int64_t gday_opus_position(GdayOpusFile *file);
uint64_t gday_opus_bytes_read(GdayOpusFile *file);
int gday_opus_seek(GdayOpusFile *file, int64_t frame);
int gday_opus_read(GdayOpusFile *file, float *samples, int capacity);

// Single producer / single consumer. All tracks share one pair of cursors.
// No locks, allocation, decoding or file I/O on the render thread.
typedef struct GdayPlaybackRing GdayPlaybackRing;
GdayPlaybackRing *gday_playback_create(uint32_t tracks, uint32_t capacity);
void gday_playback_destroy(GdayPlaybackRing *ring);
uint32_t gday_playback_available(GdayPlaybackRing *ring);
uint32_t gday_playback_free(GdayPlaybackRing *ring);
void gday_playback_write_track(GdayPlaybackRing *ring, uint32_t track, const float *left, const float *right, uint32_t frames);
void gday_playback_commit(GdayPlaybackRing *ring, uint32_t frames);
void gday_playback_set_audible(GdayPlaybackRing *ring, uint32_t mask);
uint32_t gday_playback_render(GdayPlaybackRing *ring, float *left, float *right, uint32_t frames);
uint64_t gday_playback_consumed(GdayPlaybackRing *ring);
uint64_t gday_playback_underruns(GdayPlaybackRing *ring);
// Reset only with both engine and producer stopped.
void gday_playback_reset(GdayPlaybackRing *ring);
#endif
