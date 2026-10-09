"""Apply the hero clip's look to a Cap Studio project (project-config.json).

usage: cap-config.py <project.cap> <wallpaper.jpg>

Trims the take, adds two auto zooms (typing a reply, searching), turns on
Cap's cursor smoothing, motion blur and click ripple, and puts the window on
the wallpaper with padding, rounded corners and a shadow, in 16:9. Times are
seconds into the recording and follow tour.sh's pacing.
"""
import json
import sys

project, wallpaper = sys.argv[1], sys.argv[2]
path = f"{project}/project-config.json"
config = json.load(open(path))
TRIM_START, TRIM_END = 2.0, 53.6


def zoom(start, end, amount):
    return {
        "start": start - TRIM_START,
        "end": end - TRIM_START,
        "amount": amount,
        "mode": "auto",
        "glideDirection": "none",
        "glideSpeed": 0.5,
        "instantAnimation": False,
        "edgeSnapRatio": 0.25,
    }


config["aspectRatio"] = "wide"
config["timeline"]["segments"] = [
    {"recordingSegment": 0, "timescale": 1.0, "start": TRIM_START, "end": TRIM_END, "name": None}
]
config["timeline"]["zoomSegments"] = [zoom(11.3, 17.4, 1.35), zoom(29.6, 33.4, 1.45)]
cursor = config["cursor"]
cursor.update({"size": 115, "motionBlur": 1.0})
cursor["ripple"].update({"enabled": True, "strength": 0.45, "color": [255, 255, 255]})
config["background"].update(
    {
        "source": {"type": "image", "path": wallpaper},
        "padding": 9.0,
        "rounding": 14.0,
        "shadow": 60.0,
        "advancedShadow": {"size": 40.0, "opacity": 55.0, "blur": 45.0},
    }
)
json.dump(config, open(path, "w"), indent=2)
