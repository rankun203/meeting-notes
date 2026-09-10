"""Deterministic synthetic tones for the isolated design demo, not meeting audio."""
from array import array
from math import pi, sin
import sys
import wave


def write_demo_audio(path, seconds=480, phase=0):
    rate = 8000
    block = rate // 5
    carrier = [sin(2 * pi * (180 + phase * 40) * i / rate) for i in range(block)]
    with wave.open(str(path), 'wb') as audio:
        audio.setnchannels(1)
        audio.setsampwidth(2)
        audio.setframerate(rate)
        for step in range(seconds * 5):
            t = step / 5
            amplitude = (0.12 + 0.45 * abs(sin(t * 0.47 + phase))) * abs(sin(t * 1.71 + phase))
            if (t + phase * 3) % 13 > 10:
                amplitude = 0
            samples = array('h', (round(value * amplitude * 32767) for value in carrier))
            if sys.byteorder != 'little':
                samples.byteswap()
            audio.writeframesraw(samples.tobytes())
