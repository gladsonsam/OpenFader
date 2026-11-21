#!/usr/bin/env python3
"""Repeatable Linux desktop capture for visual QA.

Runs the built OpenFader Linux bundle on an isolated Xvfb display with its own
HOME/XDG state, optionally enters the offline mixer, resizes the window and
writes one cropped PNG per requested size. This is a capture tool, not a test
suite: a person must still inspect the images against the visual standards in
AGENTS.md.

Requirements: Xvfb, xdotool, scrot, dbus-run-session and a built Linux bundle.
Use --build to build the native host and the Flutter bundle first.
"""
import argparse
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_BUNDLE = ROOT / "app" / "build" / "linux" / "x64" / "debug" / "bundle"
DEFAULT_SIZES = "1280x800,1024x700,900x640"
DEFAULT_OUT = Path("/tmp/openfader-visual-qa")


def fail(message):
    print(f"visual_qa: {message}", file=sys.stderr)
    return 1


def parse_sizes(value):
    sizes = []
    for raw in value.split(","):
        raw = raw.strip()
        if not raw:
            continue
        match = re.fullmatch(r"(\d+)x(\d+)", raw)
        if not match:
            raise argparse.ArgumentTypeError(
                f"invalid size {raw!r}; use WIDTHxHEIGHT, e.g. 900x640"
            )
        width, height = (int(match.group(1)), int(match.group(2)))
        if width < 480 or height < 480:
            raise argparse.ArgumentTypeError(
                f"size {raw!r} is too small for a useful desktop capture"
            )
        sizes.append((width, height))
    if not sizes:
        raise argparse.ArgumentTypeError("provide at least one WIDTHxHEIGHT")
    return sizes


def find_flutter():
    candidate = os.environ.get("FLUTTER") or shutil.which("flutter")
    if not candidate:
        raise SystemExit(
            "visual_qa: Flutter is not on PATH; set FLUTTER=/path/to/flutter"
        )
    return candidate


def build(bundle):
    """Build the native host and Linux bundle with the engine next to the app."""
    env = os.environ.copy()
    subprocess.run(
        [
            "cargo",
            "build",
            "--locked",
            "-p",
            "openfader-serve",
            "-p",
            "openfader-audio",
        ],
        cwd=ROOT,
        env=env,
        check=True,
    )
    metadata = subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=ROOT,
        text=True,
    )
    env["CARGO_TARGET_DIR"] = json.loads(metadata)["target_directory"]
    flutter = find_flutter()
    subprocess.run(
        [flutter, "build", "linux", "--debug"],
        cwd=ROOT / "app",
        env=env,
        check=True,
    )
    if not (bundle / "openfader").exists():
        raise SystemExit(f"visual_qa: build did not create {bundle / 'openfader'}")


def tools_available():
    missing = [
        name
        for name in ("Xvfb", "xdotool", "scrot", "dbus-run-session")
        if shutil.which(name) is None
    ]
    if missing:
        raise SystemExit(
            "visual_qa: missing required tools: " + ", ".join(missing)
        )


def free_display():
    for number in range(99, 200):
        if not Path(f"/tmp/.X11-unix/X{number}").exists():
            return number
    raise SystemExit("visual_qa: no free X display between :99 and :199")


def window_geometry(display, window_id):
    out = subprocess.check_output(
        ["xdotool", "getwindowgeometry", "--shell", str(window_id)],
        env={"DISPLAY": display},
        text=True,
    )
    values = dict(
        line.split("=", 1) for line in out.splitlines() if "=" in line
    )
    return {
        "x": int(values.get("X", 0)),
        "y": int(values.get("Y", 0)),
        "width": int(values.get("WIDTH", 0)),
        "height": int(values.get("HEIGHT", 0)),
    }


def find_window(display, timeout):
    deadline = time.time() + timeout
    while time.time() < deadline:
        proc = subprocess.run(
            ["xdotool", "search", "--name", "OpenFader"],
            env={"DISPLAY": display},
            capture_output=True,
            text=True,
        )
        for line in proc.stdout.splitlines():
            try:
                window_id = int(line)
            except ValueError:
                continue
            geometry = window_geometry(display, window_id)
            if geometry["width"] >= 480 and geometry["height"] >= 480:
                return window_id, geometry
        time.sleep(0.2)
    return None, None


def capture(display, window_id, path, attempts=3):
    geometry = window_geometry(display, window_id)
    area = "{x},{y},{width},{height}".format(**geometry)
    for attempt in range(attempts):
        subprocess.run(
            ["scrot", "-o", "-a", area, str(path)],
            env={"DISPLAY": display},
            check=True,
        )
        # A resize can briefly leave the GL surface blank; a real screen is
        # larger than the few-KB PNG that a black surface produces.
        if path.stat().st_size >= 6000 or attempt == attempts - 1:
            break
        time.sleep(0.6)
    return geometry


def resize(display, window_id, size, settle):
    subprocess.run(
        ["xdotool", "windowsize", "--sync", str(window_id), *map(str, size)],
        env={"DISPLAY": display},
        check=True,
    )
    time.sleep(settle)


def click(display, window_id, x, y):
    subprocess.run(
        [
            "xdotool",
            "mousemove",
            "--window",
            str(window_id),
            str(x),
            str(y),
            "click",
            "1",
        ],
        env={"DISPLAY": display},
        check=True,
    )


def park_mouse(display, x, y):
    """Move the pointer off the window so hover tooltips stay out of captures."""
    subprocess.run(
        ["xdotool", "mousemove", str(x), str(y)],
        env={"DISPLAY": display},
        check=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--bundle",
        type=Path,
        default=DEFAULT_BUNDLE,
        help=f"Linux bundle directory (default: {DEFAULT_BUNDLE})",
    )
    parser.add_argument(
        "--build",
        action="store_true",
        help="build the native host and Linux bundle before capturing",
    )
    parser.add_argument(
        "--sizes",
        type=parse_sizes,
        default=parse_sizes(DEFAULT_SIZES),
        help=f"comma-separated WIDTHxHEIGHT captures (default: {DEFAULT_SIZES})",
    )
    parser.add_argument(
        "--offline",
        action="store_true",
        help="click Work offline and capture the mixer instead of Connections",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=DEFAULT_OUT,
        help=f"capture directory (default: {DEFAULT_OUT})",
    )
    parser.add_argument(
        "--settle",
        type=float,
        default=0.8,
        help="seconds to wait after a resize (default: 0.8)",
    )
    parser.add_argument(
        "--boot-wait",
        type=float,
        default=6.0,
        help="seconds to wait for the Connections screen before --offline click",
    )
    parser.add_argument(
        "--startup",
        type=float,
        default=15.0,
        help="seconds to wait for the first window (default: 15)",
    )
    args = parser.parse_args()

    tools_available()
    bundle = args.bundle.resolve()
    binary = bundle / "openfader"
    if args.build:
        try:
            build(bundle)
        except subprocess.CalledProcessError as error:
            return fail(f"build failed ({error}). Fix the build and rerun.")
    if not binary.exists():
        return fail(
            f"missing {binary}; run with --build or build the Linux app first"
        )

    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    state = out / "state"
    shutil.rmtree(state, ignore_errors=True)
    for name in ("home", "data", "config", "cache"):
        (state / name).mkdir(parents=True, exist_ok=True)

    display_number = free_display()
    display = f":{display_number}"
    screen_width = max(1280, max(width for width, _ in args.sizes)) + 100
    screen_height = max(800, max(height for _, height in args.sizes)) + 100
    xvfb = subprocess.Popen(
        [
            "Xvfb",
            display,
            "-screen",
            "0",
            f"{screen_width}x{screen_height}x24",
            "-nolisten",
            "tcp",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )
    time.sleep(1.0)

    env = os.environ.copy()
    env.update(
        {
            "DISPLAY": display,
            "GDK_BACKEND": "x11",
            "HOME": str(state / "home"),
            "XDG_DATA_HOME": str(state / "data"),
            "XDG_CONFIG_HOME": str(state / "config"),
            "XDG_CACHE_HOME": str(state / "cache"),
            "NO_AT_BRIDGE": "1",
        }
    )
    log_path = out / "app.log"
    log = log_path.open("w", encoding="utf-8")
    app = subprocess.Popen(
        ["dbus-run-session", "--", str(binary)],
        cwd=bundle,
        env=env,
        stdout=log,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )

    try:
        window_id, geometry = find_window(display, args.startup)
        if window_id is None:
            log.close()
            lines = log_path.read_text(
                encoding="utf-8", errors="replace"
            ).splitlines()
            tail = "\n".join(lines[-12:])
            return fail(
                "the app window did not appear. Check the GL/EGL stack and "
                f"{log_path}\n{tail}"
            )
        print(
            "visual_qa: window {}x{} on {} (isolated state in {})".format(
                geometry["width"], geometry["height"], display, state
            )
        )

        if args.offline:
            # The Connections front door puts Work offline near the lower edge,
            # centred. The staged boot must reach that screen before it is a
            # real target.
            time.sleep(max(args.boot_wait, args.settle))
            # The runner opens at 1280x720 and the app restores its own bounds
            # after the first frame, so read the geometry again before clicking.
            geometry = window_geometry(display, window_id)
            click(
                display,
                window_id,
                geometry["width"] // 2,
                round(geometry["height"] * 0.93),
            )
            time.sleep(args.settle * 2)
            park_mouse(display, screen_width - 5, screen_height - 5)

        for width, height in args.sizes:
            resize(display, window_id, (width, height), args.settle)
            park_mouse(display, screen_width - 5, screen_height - 5)
            path = out / f"{width}x{height}.png"
            geometry = capture(display, window_id, path)
            actual = (geometry["width"], geometry["height"])
            if actual != (width, height):
                renamed = out / f"{actual[0]}x{actual[1]}.png"
                path.replace(renamed)
                path = renamed
                print(
                    f"visual_qa: requested {width}x{height}; window is "
                    f"{actual[0]}x{actual[1]} (minimum size or constraints)"
                )
            print(f"visual_qa: {path} ({geometry['width']}x{geometry['height']})")
    finally:
        try:
            os.killpg(app.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            app.wait(timeout=5)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(app.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        log.close()
        try:
            os.killpg(xvfb.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            xvfb.wait(timeout=5)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(xvfb.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

    print(f"visual_qa: captures in {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
