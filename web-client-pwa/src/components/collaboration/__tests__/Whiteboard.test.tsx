import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { Whiteboard } from "../Whiteboard";

describe("Whiteboard Component", () => {
  beforeEach(() => {
    // Polyfill HTMLCanvasElement getContext
    if (typeof HTMLCanvasElement !== "undefined") {
      HTMLCanvasElement.prototype.getContext = vi.fn().mockReturnValue({
        clearRect: vi.fn(),
        beginPath: vi.fn(),
        moveTo: vi.fn(),
        lineTo: vi.fn(),
        stroke: vi.fn(),
        strokeRect: vi.fn(),
        arc: vi.fn(),
        fill: vi.fn(),
        save: vi.fn(),
        restore: vi.fn(),
        fillText: vi.fn(),
      });
    }
  });

  it("renders whiteboard canvas and tool buttons", () => {
    render(<Whiteboard participantName="Mahdi" />);

    expect(screen.getByTestId("whiteboard")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /pen tool/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /eraser tool/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /laser pointer/i })).toBeInTheDocument();
    expect(screen.getByText("1 / 1")).toBeInTheDocument();
  });

  it("handles tool selection", () => {
    render(<Whiteboard participantName="Mahdi" />);

    const laserBtn = screen.getByRole("button", { name: /laser pointer/i });
    fireEvent.click(laserBtn);
    expect(laserBtn.className).toContain("bg-red-600");

    const penBtn = screen.getByRole("button", { name: /pen tool/i });
    fireEvent.click(penBtn);
    expect(penBtn.className).toContain("bg-emerald-600");
  });

  it("handles slide navigation and adds new slides", () => {
    const onSendData = vi.fn().mockResolvedValue(undefined);
    render(<Whiteboard participantName="Mahdi" isHost={true} onSendData={onSendData} />);

    // Add slide
    const addSlideBtn = screen.getByRole("button", { name: /add new slide/i });
    fireEvent.click(addSlideBtn);

    expect(screen.getByText("2 / 2")).toBeInTheDocument();
    expect(onSendData).toHaveBeenCalled();

    // Previous slide
    const prevSlideBtn = screen.getByRole("button", { name: /previous slide/i });
    fireEvent.click(prevSlideBtn);
    expect(screen.getByText("1 / 2")).toBeInTheDocument();
  });

  it("clears current slide and broadcasts action", () => {
    const onSendData = vi.fn().mockResolvedValue(undefined);
    render(<Whiteboard participantName="Mahdi" onSendData={onSendData} />);

    const clearBtn = screen.getByRole("button", { name: /clear current slide/i });
    fireEvent.click(clearBtn);

    expect(onSendData).toHaveBeenCalled();
  });
});
