"""Render restrained, stationary violet illumination for the Arch background.

Requires Python 3, numpy, Pillow, ffmpeg, and ffprobe. Run from any directory:
    python3 render_arch_light.py

The 16-second loop uses periodic light fields, no crossfade or moving geometry.
Only existing violet pixels change. No new glow is painted into the black centre.
The 1672x941 source is resampled to exact 1920x1080 for video compatibility.

Teams' Add new supports JPG/PNG/BMP, not custom video uploads. For motion,
composite a segmented camera above this looping clip in a virtual-camera tool,
then select that virtual camera in Teams and leave Teams effects off. Merely
playing the clip in a virtual camera will show the clip without your face.
The included PNG works as a normal static Teams background.
"""

from pathlib import Path
import hashlib
import json
import subprocess

import numpy as np
from PIL import Image, ImageDraw


HERE = Path(__file__).resolve().parent
SOURCE = HERE / "../../wallpapers/code-villain-arch-cloak.png"
STEM = "code-villain-arch-light"
FPS = 30
DURATION = 16
SIZE = (1920, 1080)


def prepare():
    source = Image.open(SOURCE).convert("RGB")
    srgb = np.asarray(source).astype(np.float32) / 255
    linear = np.where(srgb <= .04045, srgb / 12.92, ((srgb + .055) / 1.055) ** 2.4)
    r, g, b = srgb.transpose(2, 0, 1)
    # Violet confidence: neutral masonry and the centre receive exactly zero.
    chroma = np.minimum(r, b) - g
    mask = np.clip((chroma - .012) / .075, 0, 1)
    mask = mask * mask * (3 - 2 * mask)
    y, x = np.mgrid[0:source.height, 0:source.width].astype(np.float32)
    x /= source.width
    y /= source.height
    # Broad offset fields pass across the two sides at different times.
    phase = 2.2 * np.arctan2(y - 1.05, (x - .5) * 1.55)
    secondary_phase = 2 * np.pi * (.65 * y + .18 * x)
    values = np.arange(65536, dtype=np.float32) / 65535
    lut = np.rint(255 * np.where(values <= .0031308, values * 12.92,
                                1.055 * values ** (1 / 2.4) - .055)).astype(np.uint8)

    def frame(seconds):
        t = 2 * np.pi * (seconds % DURATION) / DURATION
        light = .25 * np.cos(t - phase) + .065 * np.cos(2 * t + secondary_phase)
        gain = 1 + mask * light
        indices = np.rint(np.clip(linear * gain[..., None], 0, 1) * 65535).astype(np.uint16)
        return lut[indices]

    return source, mask, frame


def main():
    source, mask, frame = prepare()
    out = HERE / f"{STEM}.mp4"
    command = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y",
               "-f", "rawvideo", "-pixel_format", "rgb24", "-video_size",
               f"{source.width}x{source.height}", "-framerate", str(FPS), "-i", "pipe:0",
               "-vf", "scale=1920:1080:flags=lanczos,setsar=1", "-an",
               "-c:v", "libx264", "-preset", "slow", "-crf", "10", "-tune", "stillimage",
               "-x264-params", f"keyint={FPS * DURATION}:min-keyint={FPS * DURATION}:scenecut=0",
               "-pix_fmt", "yuv420p", "-profile:v", "high", "-level:v", "4.1",
               "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709",
               "-movflags", "+faststart", str(out)]
    proc = subprocess.Popen(command, stdin=subprocess.PIPE)
    try:
        for i in range(FPS * DURATION):
            proc.stdin.write(frame(i / FPS).tobytes())
            if i % FPS == 0:
                print(f"Rendered {i // FPS}/{DURATION} seconds", flush=True)
    finally:
        proc.stdin.close()
    if proc.wait():
        raise RuntimeError("ffmpeg render failed")

    Image.fromarray(frame(0)).resize(SIZE, Image.Resampling.LANCZOS).save(HERE / f"{STEM}.png")
    # Contact sheet is a QA intermediate, kept outside the repository.
    sheet = Image.new("RGB", (960, 580), "#0b0b0b")
    draw = ImageDraw.Draw(sheet)
    for i, t in enumerate((0, 4, 8, 12)):
        preview = Image.fromarray(frame(t)).resize((480, 270), Image.Resampling.LANCZOS)
        x, y = (i % 2) * 480, (i // 2) * 290
        sheet.paste(preview, (x, y))
        draw.text((x + 8, y + 273), f"{t:02d}s", fill="#cccccc")
    sheet.save("/private/tmp/code-villain-arch-light-contact.jpg", quality=95)

    first = frame(0).astype(np.int16)
    last = frame(DURATION - 1 / FPS).astype(np.int16)
    next_frame = frame(1 / FPS).astype(np.int16)
    metadata = {
        "source": "../../wallpapers/code-villain-arch-cloak.png",
        "source_sha256": hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        "source_size": [source.width, source.height],
        "output": out.name,
        "size": list(SIZE), "fps": FPS, "duration_seconds": DURATION,
        "frames": FPS * DURATION,
        "design": "Slow soft violet light passing around fixed arch facets. No camera motion, text, strike, or sound.",
        "loop": "Analytically periodic light fields. Frame at 16s is identical to frame at 0s; duplicate endpoint omitted.",
        "validation": {
            "exact_periodic_endpoint": bool(np.array_equal(first, frame(DURATION))),
            "source_frame_seam_mean_absolute_code_difference": float(np.abs(first - last).mean()),
            "source_frame_next_mean_absolute_code_difference": float(np.abs(first - next_frame).mean()),
            "neutral_pixels_fixed": bool(np.array_equal(frame(0)[mask == 0], frame(8)[mask == 0])),
        },
        "teams": {
            "checked": "2026-10-19",
            "native_custom_video_upload": False,
            "usage": "Loop this MP4 behind your segmented camera in a virtual-camera compositor; select its virtual camera in Teams and leave Teams background effects off. The PNG can be uploaded as a static background.",
            "sources": [
                "https://support.microsoft.com/en-gb/teams/meetings/change-your-background-in-microsoft-teams-meetings",
                "https://techcommunity.microsoft.com/blog/microsoft365insiderblog/animated-backgrounds-in-teams-meetings/4220884"
            ],
        },
        "rebuild": "python3 render_arch_light.py (requires numpy, Pillow, ffmpeg and ffprobe)",
    }
    metadata["ffprobe"] = json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-show_entries",
        "stream=codec_name,profile,width,height,pix_fmt,r_frame_rate,nb_frames,color_space,color_transfer,color_primaries:format=duration,size",
        "-of", "json", str(out)]))
    (HERE / f"{STEM}.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(json.dumps(metadata["validation"], indent=2))


if __name__ == "__main__":
    main()
