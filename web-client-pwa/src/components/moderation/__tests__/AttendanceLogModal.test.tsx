import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { AttendanceLogModal, AttendanceRecord } from "../AttendanceLogModal";

describe("AttendanceLogModal Component", () => {
  const records: AttendanceRecord[] = [
    {
      participantId: "p-1",
      name: "Dr. Smith",
      identity: "smith",
      role: "host",
      joinedAt: Date.now() - 3600000,
      durationSeconds: 3600,
      connectionQuality: "excellent",
    },
    {
      participantId: "p-2",
      name: "Student Jane",
      identity: "jane",
      role: "speaker",
      joinedAt: Date.now() - 1800000,
      leftAt: Date.now() - 300000,
      durationSeconds: 1500,
      connectionQuality: "good",
    },
  ];

  beforeEach(() => {
    URL.createObjectURL = vi.fn().mockReturnValue("blob:mock-csv-url");
    URL.revokeObjectURL = vi.fn();
    HTMLAnchorElement.prototype.click = vi.fn();
  });

  it("renders participant records and filters by search query", () => {
    render(
      <AttendanceLogModal
        isOpen={true}
        onClose={vi.fn()}
        roomName="Math101"
        records={records}
      />
    );

    expect(screen.getByTestId("attendance-log-modal")).toBeInTheDocument();
    expect(screen.getByText("Dr. Smith")).toBeInTheDocument();
    expect(screen.getByText("Student Jane")).toBeInTheDocument();

    const searchInput = screen.getByPlaceholderText(/search participants/i);
    fireEvent.change(searchInput, { target: { value: "jane" } });

    expect(screen.queryByText("Dr. Smith")).not.toBeInTheDocument();
    expect(screen.getByText("Student Jane")).toBeInTheDocument();
  });

  it("handles CSV and JSON export clicks", () => {
    render(
      <AttendanceLogModal
        isOpen={true}
        onClose={vi.fn()}
        roomName="Math101"
        records={records}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: /export csv/i }));
    expect(URL.createObjectURL).toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: /export json/i }));
    expect(URL.createObjectURL).toHaveBeenCalledTimes(2);
  });
});
