import { loadPhysics, stepBody } from "./simulation.mjs";

const canvas = document.querySelector("#game");
const context = canvas.getContext("2d");
const status = document.querySelector("#status");
const pause = document.querySelector("#pause");
const count = document.querySelector("#count");
const countLabel = document.querySelector("#count-label");
const fpsLabel = document.querySelector("#fps");

function showError(error) {
  console.error(error);
  status.textContent = error instanceof Error ? error.message : String(error);
  status.className = "error";
  pause.disabled = true;
  count.disabled = true;
}

async function start() {
  if (!context) throw new Error("This browser does not provide a Canvas 2D context.");
  const api = await loadPhysics(new URL("./physics.wasm", import.meta.url));
  let paused = matchMedia("(prefers-reduced-motion: reduce)").matches;
  let bodies = [];
  let previous;
  let sampleStart;
  let frames = 0;

  function reset() {
    bodies = Array.from({ length: Number(count.value) }, (_, index) => ({
      x: 30 + (index * 137) % (canvas.width - 60),
      y: 30 + (index * 97) % (canvas.height - 60),
      vx: (index % 2 ? -1 : 1) * (90 + index % 7 * 22),
      vy: (index % 3 ? 1 : -1) * (70 + index % 5 * 27),
      radius: 8 + index % 6,
      hue: (165 + index * 23) % 360,
    }));
    countLabel.textContent = String(bodies.length);
  }

  function updatePause() {
    pause.textContent = paused ? "Resume" : "Pause";
    pause.setAttribute("aria-pressed", String(paused));
    status.textContent = paused ? "Paused. All physics runs in LLVM-generated WASM." : "Running. UI and drawing stay in JavaScript.";
  }

  pause.addEventListener("click", () => { paused = !paused; updatePause(); });
  count.addEventListener("input", reset);
  pause.disabled = false;
  count.disabled = false;
  reset();
  updatePause();

  function frame(time) {
    try {
      const dt = previous === undefined ? 0 : Math.min((time - previous) / 1000, 0.05);
      previous = time;
      if (!paused) {
        bodies = bodies.map((body) => stepBody(api, body, dt, canvas.width, canvas.height));
      }
      context.clearRect(0, 0, canvas.width, canvas.height);
      for (const body of bodies) {
        context.beginPath();
        context.arc(body.x, body.y, body.radius, 0, Math.PI * 2);
        context.fillStyle = `hsl(${body.hue} 80% 65%)`;
        context.fill();
      }
      frames++;
      if (sampleStart === undefined) sampleStart = time;
      if (time - sampleStart >= 500) {
        fpsLabel.textContent = `${Math.round(frames * 1000 / (time - sampleStart))} FPS`;
        frames = 0;
        sampleStart = time;
      }
      requestAnimationFrame(frame);
    } catch (error) {
      showError(error);
    }
  }
  requestAnimationFrame(frame);
}

start().catch(showError);
