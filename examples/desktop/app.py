"""A Tk desktop host for the same pure kernel used by the browser demo."""

import argparse
import ctypes
import json
import time
from pathlib import Path


class Physics:
    def __init__(self, library: Path):
        self.library = ctypes.CDLL(str(library.resolve()))
        self.position = self.library.tz_next_position
        self.velocity = self.library.tz_next_velocity
        for function in (self.position, self.velocity):
            function.argtypes = [ctypes.c_double] * 4
            function.restype = ctypes.c_double

    def step(self, position: float, velocity: float, dt: float, extent: float):
        return (
            self.position(position, velocity, dt, extent),
            self.velocity(position, velocity, dt, extent),
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("library", type=Path, help="LLVM-built physics .dylib, .so, or .dll")
    parser.add_argument("--headless", action="store_true", help="Run 600 deterministic steps without Tk")
    arguments = parser.parse_args()
    try:
        physics = Physics(arguments.library)
    except (OSError, AttributeError) as error:
        parser.error(f"cannot load the Tsuzuri physics ABI: {error}")

    x, y, vx, vy = 100.0, 50.0, 120.0, 90.0
    if arguments.headless:
        for _ in range(600):
            x, vx = physics.step(x, vx, 1.0 / 60.0, 640.0)
            y, vy = physics.step(y, vy, 1.0 / 60.0, 360.0)
        print(json.dumps({"x": x, "y": y, "vx": vx, "vy": vy}, allow_nan=False))
        return

    try:
        import tkinter as tk
    except ImportError as error:
        raise SystemExit("The GUI needs Python with Tk support; use --headless to check the native ABI.") from error

    try:
        window = tk.Tk()
    except tk.TclError as error:
        raise SystemExit(f"Cannot open a desktop display: {error}. Use --headless without a display.") from error
    window.title("Tsuzuri / native LLVM physics")
    canvas = tk.Canvas(window, width=664, height=384, bg="#17242d", highlightthickness=0)
    canvas.pack(padx=16, pady=16)
    ball = canvas.create_oval(0, 0, 24, 24, fill="#69e8c1", outline="")
    paused = False
    previous = time.perf_counter()

    def toggle():
        nonlocal paused
        paused = not paused
        button.configure(text="Resume" if paused else "Pause")

    button = tk.Button(window, text="Pause", command=toggle)
    button.pack(pady=(0, 16))

    def tick():
        nonlocal x, y, vx, vy, previous
        now = time.perf_counter()
        dt = min(now - previous, 0.05)
        previous = now
        if not paused:
            x, vx = physics.step(x, vx, dt, 640.0)
            y, vy = physics.step(y, vy, dt, 360.0)
        canvas.coords(ball, x, y, x + 24, y + 24)
        window.after(16, tick)

    tick()
    window.mainloop()


if __name__ == "__main__":
    main()
