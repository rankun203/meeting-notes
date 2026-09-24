#ifndef GDAY_AUDIO_CAPTURE_BRIDGE_H
#define GDAY_AUDIO_CAPTURE_BRIDGE_H
#include <CoreAudio/CoreAudio.h>
#include <stdbool.h>
#include <stdint.h>
typedef struct GdayAudioRing GdayAudioRing;
GdayAudioRing * _Nullable GdayAudioRingCreate(uint32_t channels, bool interleaved, uint32_t maxFrames, uint32_t capacity);
void GdayAudioRingDestroy(GdayAudioRing * _Nullable ring);
bool GdayAudioRingPush(GdayAudioRing * _Nullable ring, const AudioBufferList * _Nullable input, const AudioTimeStamp * _Nullable time);
bool GdayAudioRingRead(GdayAudioRing * _Nullable ring, float * _Nullable destination, uint32_t capacityFrames, uint32_t * _Nullable frames, uint64_t * _Nullable hostTime);
int GdayAudioRingFailure(GdayAudioRing * _Nullable ring);
OSStatus GdayAudioIOProc(AudioObjectID device, const AudioTimeStamp * _Nonnull now, const AudioBufferList * _Nonnull input, const AudioTimeStamp * _Nonnull inputTime, AudioBufferList * _Nonnull output, const AudioTimeStamp * _Nonnull outputTime, void * _Nullable context);
#endif
