"""Animate violet light across stationary cube surfaces, with a fixed poison strike.

Requires Python 3, numpy, Pillow, ffmpeg and ffprobe. Run:
    python3 render_cubes_light.py

Teams accepts custom still images, not custom MP4 uploads. Composite the looping
video behind a segmented webcam using a virtual-camera tool to use the motion.
The GIF is a motion preview, not a Teams upload format.
"""
from pathlib import Path
import hashlib
import json
import subprocess

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

HERE = Path(__file__).resolve().parent
SOURCE = HERE / "../code-villain-cubes.png"
STEM = "code-villain-cubes-light"
FPS, DURATION = 30, 8


def prepare():
    source = Image.open(SOURCE).convert("RGB")
    base = np.asarray(source).astype(np.float32) / 255
    linear = np.where(base <= .04045, base / 12.92, ((base + .055) / 1.055) ** 2.4)
    w, h = source.size
    # Trace the actual cube silhouettes so light never spills into the centre.
    silhouettes = Image.new("L", source.size)
    draw = ImageDraw.Draw(silhouettes)
    draw.polygon([(0, 389), (307, 490), (309, 702), (343, 680),
                  (458, 724), (458, 849), (596, 941), (0, 941)], fill=255)
    draw.polygon([(1385, 65), (1522, 0), (1672, 0), (1672, 941),
                  (1434, 818), (1434, 671), (1265, 540), (1260, 320),
                  (1385, 260)], fill=255)
    mask = np.asarray(silhouettes.filter(ImageFilter.GaussianBlur(.55))).astype(np.float32) / 255
    y, x = np.mgrid[0:h, 0:w].astype(np.float32)
    x, y = x / w, y / h
    # Separate phase on each side gives distinct travelling highlights.
    phase = 2 * np.pi * (1.16 * y + np.where(x < .5, .18 * x, -.22 * x + .28))
    reflectance = .06 + .94 * np.sqrt(base.max(axis=2))
    violet = np.array([.11, .012, .245], dtype=np.float32)
    values = np.arange(65536, dtype=np.float32) / 65535
    lut = np.rint(255 * np.where(values <= .0031308, values * 12.92,
                                1.055 * values ** (1 / 2.4) - .055)).astype(np.uint8)

    def frame(seconds):
        t = 2 * np.pi * (seconds % DURATION) / DURATION
        wave = ((1 + np.cos(t - phase)) / 2) ** 4
        # Actual added light, rather than a tiny gain on already-dark pixels.
        lighting = mask * wave
        lit = linear * (1 + .5 * lighting[..., None])
        lit += (lighting * reflectance)[..., None] * violet
        return lut[np.rint(np.clip(lit, 0, 1) * 65535).astype(np.uint16)]

    return source, mask, frame


def main():
    source, mask, frame = prepare()
    out = HERE / f"{STEM}.mp4"
    proc = subprocess.Popen([
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo",
        "-pixel_format", "rgb24", "-video_size", f"{source.width}x{source.height}",
        "-framerate", str(FPS), "-i", "pipe:0", "-vf",
        "scale=1920:1080:flags=lanczos:out_color_matrix=bt709,setsar=1", "-an",
        "-c:v", "libx264", "-preset", "slow", "-crf", "12", "-tune", "stillimage",
        "-pix_fmt", "yuv420p", "-profile:v", "high", "-level:v", "4.1",
        "-colorspace", "bt709", "-movflags", "+faststart", str(out)
    ], stdin=subprocess.PIPE)
    try:
        for i in range(FPS * DURATION):
            proc.stdin.write(frame(i / FPS).tobytes())
            if i % FPS == 0:
                print(f"Rendered {i // FPS}/{DURATION}s", flush=True)
    finally:
        proc.stdin.close()
    if proc.wait():
        raise RuntimeError("ffmpeg failed")
    Image.fromarray(frame(1)).resize((1920, 1080), Image.Resampling.LANCZOS).save(HERE / f"{STEM}.png")
    # Fixed global palette prevents unrelated background noise from flickering.
    samples = [Image.fromarray(frame(t)).resize((640, 360), Image.Resampling.LANCZOS)
               for t in (0, 1, 2, 3, 4, 5, 6, 7)]
    palette_strip = Image.new("RGB", (640 * len(samples), 360))
    for i, sample in enumerate(samples):
        palette_strip.paste(sample, (i * 640, 0))
    palette = palette_strip.quantize(colors=256)
    preview = [Image.fromarray(frame(i / 10)).resize((640, 360), Image.Resampling.LANCZOS)
               .quantize(palette=palette, dither=Image.Dither.NONE) for i in range(DURATION * 10)]
    preview[0].save(HERE / f"{STEM}-preview.gif", save_all=True,
                    append_images=preview[1:], duration=100, loop=0, optimize=True)
    sheet = Image.new("RGB", (1280, 760), "#0b0b0b")
    draw = ImageDraw.Draw(sheet)
    for i, t in enumerate((0, 2, 4, 6)):
        thumb = Image.fromarray(frame(t)).resize((640, 360), Image.Resampling.LANCZOS)
        x, y = (i % 2) * 640, (i // 2) * 380
        sheet.paste(thumb, (x, y))
        draw.text((x + 8, y + 363), f"{t:02d}s", fill="#cccccc")
    sheet.save("/private/tmp/code-villain-cubes-light-contact.jpg", quality=95)
    region = mask > .99
    phases = np.stack([frame(t) for t in np.arange(0, DURATION, .25)])
    blue = phases[..., 2].astype(np.float32)
    blue_range = blue.max(axis=0) - blue.min(axis=0)
    # The central background and poison stripe must be absolutely unchanged.
    unchanged = bool(np.array_equal(frame(0)[mask == 0], frame(3)[mask == 0]))
    metadata = {
        "source": "../code-villain-cubes.png", "source_sha256": hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        "source_size": list(source.size), "output": out.name,
        "size": [1920, 1080], "duration_seconds": DURATION, "fps": FPS,
        "design": "Clearly visible violet light sweeps across cube faces and engraved tops; fixed cubes, black centre and poison strike.",
        "validation": {
            "exact_periodic_endpoint": bool(np.array_equal(frame(0), frame(DURATION))),
            "outside_cube_silhouettes_fixed": unchanged,
            "cube_blue_channel_temporal_range_median_codes": float(np.median(blue_range[region])),
            "cube_blue_channel_temporal_range_p90_codes": float(np.percentile(blue_range[region], 90)),
        },
        "teams_usage": "For motion, loop the MP4 behind a segmented webcam in a virtual-camera compositor, then choose its virtual camera in Teams with Teams background effects off. PNG is the static fallback; GIF is preview only.",
        "rebuild": "python3 render_cubes_light.py (numpy, Pillow, ffmpeg, ffprobe)",
    }
    metadata["ffprobe"] = json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-show_entries",
        "stream=codec_name,profile,width,height,pix_fmt,r_frame_rate,nb_frames:format=duration,size",
        "-of", "json", str(out)]))
    (HERE / f"{STEM}.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(json.dumps(metadata["validation"], indent=2))


if __name__ == "__main__":
    main()
