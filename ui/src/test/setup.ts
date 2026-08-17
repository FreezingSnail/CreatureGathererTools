import "@testing-library/jest-dom/vitest";
import { afterEach, beforeAll } from "vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => cleanup());

// jsdom ships no canvas implementation. Stub the 2D context as a no-op proxy
// so components (including the canvas draw path) execute safely in tests.
const ctxStub = new Proxy(
  {},
  {
    get: (_t, prop) => {
      if (prop === "canvas") return {};
      if (prop === "getImageData") return () => ({ data: new Uint8ClampedArray(0) });
      return () => 0;
    },
    set: () => true,
  }
) as CanvasRenderingContext2D;

beforeAll(() => {
  HTMLCanvasElement.prototype.getContext = ((_id: string) =>
    ctxStub) as typeof HTMLCanvasElement.prototype.getContext;
  // jsdom lacks canvas; return a stub data URL so palette buttons render.
  HTMLCanvasElement.prototype.toDataURL = (() => "data:,") as typeof HTMLCanvasElement.prototype.toDataURL;
  if (typeof globalThis.ResizeObserver === "undefined") {
    class ResizeObserverStub {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
    (globalThis as { ResizeObserver?: typeof ResizeObserver }).ResizeObserver =
      ResizeObserverStub as unknown as typeof ResizeObserver;
  }
});