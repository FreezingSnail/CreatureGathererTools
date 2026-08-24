import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { LayoutView } from "./LayoutView";
import type { LayoutReport } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mockInvoke = vi.mocked(invoke) as unknown as ReturnType<typeof vi.fn>;

const report: LayoutReport = {
  data_page: 32,
  save_page: 96,
  data_len: 300,
  data_len_flash: 512,
  save_len: 20,
  save_len_flash: 4096,
  page_size: 256,
  flash_pages: 128,
  fields: [
    { name: "title", namespace: "maps", addr: 0, len: 10, span: 16, kind: "bytes", in_save: false, source: "raw", path: "maps/title.bin" },
    { name: "hero", namespace: "sprites", addr: 256, len: 290, span: 290, kind: "image", in_save: false, source: "image", path: "sprites/hero.png", image: { width: 16, height: 16, frames: 4 } },
    { name: "save_data", namespace: "save", addr: 0, len: 20, span: 32, kind: "bytes", in_save: true, source: "raw", path: "save.bin" },
  ],
};

beforeEach(() => mockInvoke.mockReset());

describe("LayoutView", () => {
  it("renders report sections, summaries, and field details", async () => {
    mockInvoke.mockResolvedValue(report);
    render(<LayoutView />);
    expect(await screen.findByTestId("layout-view")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("layout_report");
    expect(screen.getByText("FX_DATA_PAGE 32")).toBeInTheDocument();
    expect(screen.getByText("Data address space")).toBeInTheDocument();
    expect(screen.getByText("Save address space")).toBeInTheDocument();
    expect(screen.getByLabelText("Data flash page map")).toBeInTheDocument();
    expect(screen.getByLabelText("Save flash page map")).toBeInTheDocument();
    expect(screen.getByText("0x000100")).toBeInTheDocument();
    expect(screen.getByText("16×16 × 4")).toBeInTheDocument();
    expect(screen.getByText("18 B")).toBeInTheDocument();
  });

  it("synchronizes canvas and table selection", async () => {
    mockInvoke.mockResolvedValue(report);
    render(<LayoutView />);
    await screen.findByText("title");
    const canvas = screen.getByLabelText("Data flash page map");
    vi.spyOn(canvas, "getBoundingClientRect").mockReturnValue({ x: 0, y: 0, width: 384, height: 24, top: 0, left: 0, bottom: 24, right: 384, toJSON: () => ({}) });
    fireEvent.click(canvas, { clientX: 5, clientY: 5 });
    expect(screen.getByText("title").closest("tr")).toHaveAttribute("aria-selected", "true");
    expect(canvas).toHaveAttribute("data-selected-name", "title");

    fireEvent.click(screen.getByText("hero"));
    await waitFor(() => expect(screen.getByText("hero").closest("tr")).toHaveAttribute("aria-selected", "true"));
    expect(canvas).toHaveAttribute("data-selected-name", "hero");
  });

  it("shows backend errors with Compile all guidance and supports refresh", async () => {
    mockInvoke.mockRejectedValueOnce(new Error("missing builder bins"));
    mockInvoke.mockResolvedValueOnce(report);
    render(<LayoutView />);
    expect(await screen.findByTestId("layout-error")).toHaveTextContent("missing builder bins");
    expect(screen.getByTestId("layout-error")).toHaveTextContent("Compile all");
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await screen.findByTestId("layout-view")).toBeInTheDocument();
  });

  it("shows an empty report", async () => {
    mockInvoke.mockResolvedValue({ ...report, fields: [] });
    render(<LayoutView />);
    expect(await screen.findByTestId("layout-empty")).toHaveTextContent("No layout fields");
  });
});
