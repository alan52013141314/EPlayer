"""Generate continuous local signals and verify the actual decoder/resampler path."""
import argparse
import array
import json
import math
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument("--player", type=Path, required=True)
parser.add_argument("--ffmpeg", type=Path, required=True)
parser.add_argument("--report", type=Path, help="Output report path; defaults to verification/gapless-report.json")
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
out = root / "verification" / "generated" / "gapless"
out.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, EPLAYER_MEDIA_TOOLS=str(args.ffmpeg.resolve().parent))
process_flags = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0

def ff(*arguments):
    subprocess.run([str(args.ffmpeg), "-v", "error", "-y", *map(str, arguments)], check=True, capture_output=True, creationflags=process_flags)

def samples(path):
    result = array.array("f")
    result.frombytes(path.read_bytes())
    return result

reference = out / "reference.wav"
ff("-f", "lavfi", "-i", "sine=frequency=997.3:sample_rate=48000:duration=6",
   "-ac", "2", "-c:a", "pcm_f32le", reference)
reference_raw = out / "reference.raw"
ff("-i", reference, "-f", "f32le", reference_raw)
expected = samples(reference_raw)
reports = []
for extension, codec in [("wav", "pcm_f32le"), ("flac", "flac"), ("mp3", "libmp3lame"),
                         ("ogg", "libvorbis"), ("m4a", "aac"), ("opus", "libopus"),
                         ("mixed-rate", "pcm_f32le"), ("mixed-channels", "pcm_f32le")]:
    parts = []
    for index in range(2):
        path = out / f"part-{index}-{extension}.{'wav' if extension.startswith('mixed-') else extension}"
        conversion = []
        if index == 0 and extension == "mixed-rate":
            conversion = ["-ar", "44100"]
        if index == 0 and extension == "mixed-channels":
            conversion = ["-ac", "1", "-af", f"atrim=start_sample=0:end_sample=144000,asetpts=N/SR/TB,pan=mono|c0=c0"]
        ff("-i", reference, "-af", f"atrim=start_sample={index*144000}:end_sample={(index+1)*144000},asetpts=N/SR/TB",
           *conversion, "-c:a", codec, path)
        parts.append(path)
    rendered = out / f"joined-{extension}.raw"
    result = subprocess.run([str(args.player), "--render-probe", str(rendered), *map(str, parts)],
                            env=env, capture_output=True, text=True, encoding="utf-8", check=True, creationflags=process_flags)
    actual = samples(rendered)
    assert len(actual) == len(expected), (extension, len(actual), len(expected))
    seam = 144000 * 2
    window = actual[seam-512:seam+512]
    baseline = expected[seam-512:seam+512]
    energy_ratio = sum(x*x for x in window) / sum(x*x for x in baseline)
    assert 0.5 < energy_ratio < 2.0, (extension, energy_ratio)
    longest = run = 0
    for value in window:
        run = run + 1 if abs(value) < 1e-8 else 0
        longest = max(longest, run)
    assert longest <= 8, (extension, "silence inserted at boundary", longest)
    error = max(abs(a-b) for a,b in zip(actual, expected))
    if extension in ("wav", "flac"):
        assert error < 2e-7, (extension, error)
    reports.append({"format": extension, "output_frames": len(actual)//2,
                    "boundary_energy_ratio": energy_ratio, "longest_near_zero_samples": longest,
                    "max_absolute_difference": error, "render": json.loads(result.stdout)})
report = {"status": "passed", "sample_rate": 48000, "tests": reports,
          "limits": "Offline decoder/resampler boundary verification; device underruns are tested separately."}
(args.report or root / "verification" / "gapless-report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
print(json.dumps({"status": "passed", "formats": [r["format"] for r in reports]}))
