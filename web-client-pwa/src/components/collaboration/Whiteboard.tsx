"use client";

import React, { useRef, useState, useEffect, useCallback } from "react";
import {
  Pen,
  Eraser,
  Square,
  Circle as CircleIcon,
  Minus,
  Sparkles,
  Trash2,
  ChevronLeft,
  ChevronRight,
  Plus,
} from "lucide-react";

export type WhiteboardTool = "pen" | "eraser" | "line" | "rect" | "circle" | "laser";

export interface DrawPoint {
  x: number;
  y: number;
}

export interface VectorStroke {
  id: string;
  tool: WhiteboardTool;
  color: string;
  width: number;
  points: DrawPoint[];
}

export interface WhiteboardAction {
  type: "stroke" | "clear" | "page_change" | "laser";
  page?: number;
  stroke?: VectorStroke;
  laserPoint?: { x: number; y: number; participantName: string };
}

export interface WhiteboardProps {
  isHost?: boolean;
  participantName: string;
  onSendData?: (data: Uint8Array, topic: string) => Promise<void>;
  incomingData?: { data: Uint8Array; topic: string } | null;
}

export const Whiteboard: React.FC<WhiteboardProps> = ({
  isHost = true,
  participantName,
  onSendData,
  incomingData,
}) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [currentPage, setCurrentPage] = useState<number>(1);
  const [totalPages, setTotalPages] = useState<number>(1);
  const [tool, setTool] = useState<WhiteboardTool>("pen");
  const [color, setColor] = useState<string>("#3b82f6");
  const [lineWidth, setLineWidth] = useState<number>(3);
  const [isDrawing, setIsDrawing] = useState<boolean>(false);
  const [currentStroke, setCurrentStroke] = useState<DrawPoint[]>([]);
  const [pageStrokes, setPageStrokes] = useState<Record<number, VectorStroke[]>>({ 1: [] });
  const [remoteLasers, setRemoteLasers] = useState<Record<string, { x: number; y: number; name: string; time: number }>>({});

  // Broadcast helper
  const broadcastAction = useCallback(
    async (action: WhiteboardAction) => {
      if (!onSendData) return;
      try {
        const payload = new TextEncoder().encode(JSON.stringify(action));
        await onSendData(payload, "whiteboard");
      } catch {}
    },
    [onSendData]
  );

  // Redraw canvas content
  const redraw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.clearRect(0, 0, canvas.width, canvas.height);

    const strokes = pageStrokes[currentPage] || [];
    for (const stroke of strokes) {
      if (stroke.points.length === 0) continue;
      ctx.beginPath();
      ctx.strokeStyle = stroke.tool === "eraser" ? "#0f172a" : stroke.color;
      ctx.lineWidth = stroke.width;
      ctx.lineCap = "round";
      ctx.lineJoin = "round";

      if (stroke.tool === "rect" && stroke.points.length >= 2) {
        const p1 = stroke.points[0];
        const p2 = stroke.points[stroke.points.length - 1];
        ctx.strokeRect(p1.x, p1.y, p2.x - p1.x, p2.y - p1.y);
      } else if (stroke.tool === "circle" && stroke.points.length >= 2) {
        const p1 = stroke.points[0];
        const p2 = stroke.points[stroke.points.length - 1];
        const radius = Math.hypot(p2.x - p1.x, p2.y - p1.y);
        ctx.arc(p1.x, p1.y, radius, 0, 2 * Math.PI);
        ctx.stroke();
      } else if (stroke.tool === "line" && stroke.points.length >= 2) {
        const p1 = stroke.points[0];
        const p2 = stroke.points[stroke.points.length - 1];
        ctx.moveTo(p1.x, p1.y);
        ctx.lineTo(p2.x, p2.y);
        ctx.stroke();
      } else {
        // Pen / Eraser freehand
        ctx.moveTo(stroke.points[0].x, stroke.points[0].y);
        for (let i = 1; i < stroke.points.length; i++) {
          ctx.lineTo(stroke.points[i].x, stroke.points[i].y);
        }
        ctx.stroke();
      }
    }

    // Draw active stroke preview while mouse is down
    if (currentStroke.length > 0 && tool !== "laser") {
      ctx.beginPath();
      ctx.strokeStyle = tool === "eraser" ? "#0f172a" : color;
      ctx.lineWidth = lineWidth;
      ctx.lineCap = "round";
      ctx.lineJoin = "round";

      if (tool === "rect" && currentStroke.length >= 2) {
        const p1 = currentStroke[0];
        const p2 = currentStroke[currentStroke.length - 1];
        ctx.strokeRect(p1.x, p1.y, p2.x - p1.x, p2.y - p1.y);
      } else if (tool === "circle" && currentStroke.length >= 2) {
        const p1 = currentStroke[0];
        const p2 = currentStroke[currentStroke.length - 1];
        const radius = Math.hypot(p2.x - p1.x, p2.y - p1.y);
        ctx.arc(p1.x, p1.y, radius, 0, 2 * Math.PI);
        ctx.stroke();
      } else if (tool === "line" && currentStroke.length >= 2) {
        const p1 = currentStroke[0];
        const p2 = currentStroke[currentStroke.length - 1];
        ctx.moveTo(p1.x, p1.y);
        ctx.lineTo(p2.x, p2.y);
        ctx.stroke();
      } else {
        ctx.moveTo(currentStroke[0].x, currentStroke[0].y);
        for (let i = 1; i < currentStroke.length; i++) {
          ctx.lineTo(currentStroke[i].x, currentStroke[i].y);
        }
        ctx.stroke();
      }
    }

    // Draw active laser pointers
    const now = Date.now();
    for (const [, laser] of Object.entries(remoteLasers)) {
      if (now - laser.time < 3000) {
        ctx.save();
        ctx.fillStyle = "#ef4444";
        ctx.shadowColor = "#f87171";
        ctx.shadowBlur = 10;
        ctx.beginPath();
        ctx.arc(laser.x, laser.y, 6, 0, 2 * Math.PI);
        ctx.fill();

        ctx.font = "10px sans-serif";
        ctx.fillStyle = "#ffffff";
        ctx.fillText(laser.name, laser.x + 8, laser.y - 4);
        ctx.restore();
      }
    }
  }, [pageStrokes, currentPage, currentStroke, tool, color, lineWidth, remoteLasers]);

  useEffect(() => {
    redraw();
  }, [redraw]);

  // Clean expired laser pointers
  useEffect(() => {
    const timer = setInterval(() => {
      const now = Date.now();
      let changed = false;
      const nextLasers = { ...remoteLasers };
      for (const [k, v] of Object.entries(nextLasers)) {
        if (now - v.time > 3000) {
          delete nextLasers[k];
          changed = true;
        }
      }
      if (changed) setRemoteLasers(nextLasers);
    }, 1000);
    return () => clearInterval(timer);
  }, [remoteLasers]);

  // Handle incoming DataChannel messages
  useEffect(() => {
    if (!incomingData || incomingData.topic !== "whiteboard") return;
    try {
      const text = new TextDecoder().decode(incomingData.data);
      const action = JSON.parse(text) as WhiteboardAction;

      if (action.type === "stroke" && action.stroke && action.page) {
        const p = action.page;
        setPageStrokes((prev) => ({
          ...prev,
          [p]: [...(prev[p] || []), action.stroke!],
        }));
      } else if (action.type === "clear" && action.page) {
        const p = action.page;
        setPageStrokes((prev) => ({ ...prev, [p]: [] }));
      } else if (action.type === "page_change" && action.page) {
        setCurrentPage(action.page);
        if (action.page > totalPages) setTotalPages(action.page);
      } else if (action.type === "laser" && action.laserPoint) {
        setRemoteLasers((prev) => ({
          ...prev,
          [action.laserPoint!.participantName]: {
            x: action.laserPoint!.x,
            y: action.laserPoint!.y,
            name: action.laserPoint!.participantName,
            time: Date.now(),
          },
        }));
      }
    } catch {}
  }, [incomingData, totalPages]);

  // Coordinate normalizer
  const getCanvasCoords = (e: React.MouseEvent<HTMLCanvasElement>): DrawPoint => {
    const canvas = canvasRef.current;
    if (!canvas) return { x: 0, y: 0 };
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    return {
      x: (e.clientX - rect.left) * scaleX,
      y: (e.clientY - rect.top) * scaleY,
    };
  };

  const handleMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const pt = getCanvasCoords(e);
    setIsDrawing(true);
    setCurrentStroke([pt]);

    if (tool === "laser") {
      setRemoteLasers((prev) => ({
        ...prev,
        [participantName]: { x: pt.x, y: pt.y, name: participantName, time: Date.now() },
      }));
      broadcastAction({
        type: "laser",
        laserPoint: { x: pt.x, y: pt.y, participantName },
      });
    }
  };

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const pt = getCanvasCoords(e);

    if (tool === "laser") {
      setRemoteLasers((prev) => ({
        ...prev,
        [participantName]: { x: pt.x, y: pt.y, name: participantName, time: Date.now() },
      }));
      broadcastAction({
        type: "laser",
        laserPoint: { x: pt.x, y: pt.y, participantName },
      });
      return;
    }

    if (!isDrawing) return;

    if (tool === "pen" || tool === "eraser") {
      setCurrentStroke((prev) => [...prev, pt]);
    } else {
      // Line / Rect / Circle preview: keep initial anchor and current point
      setCurrentStroke((prev) => [prev[0], pt]);
    }
  };

  const handleMouseUp = () => {
    if (!isDrawing) return;
    setIsDrawing(false);

    if (tool !== "laser" && currentStroke.length > 0) {
      const stroke: VectorStroke = {
        id: `stroke-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
        tool,
        color,
        width: lineWidth,
        points: currentStroke,
      };

      setPageStrokes((prev) => ({
        ...prev,
        [currentPage]: [...(prev[currentPage] || []), stroke],
      }));

      broadcastAction({
        type: "stroke",
        page: currentPage,
        stroke,
      });
    }
    setCurrentStroke([]);
  };

  const handleClearPage = () => {
    setPageStrokes((prev) => ({ ...prev, [currentPage]: [] }));
    broadcastAction({ type: "clear", page: currentPage });
  };

  const handlePageChange = (newPage: number) => {
    if (newPage < 1) return;
    setCurrentPage(newPage);
    if (newPage > totalPages) setTotalPages(newPage);
    broadcastAction({ type: "page_change", page: newPage });
  };

  return (
    <div
      data-testid="whiteboard"
      className="flex flex-col h-full w-full bg-slate-900 border border-slate-800 rounded-xl overflow-hidden shadow-2xl select-none"
    >
      {/* Top Toolbar */}
      <div className="flex items-center justify-between px-3 py-2 bg-slate-950 border-b border-slate-800 flex-wrap gap-2 text-xs">
        {/* Drawing Tools */}
        <div className="flex items-center space-x-1">
          <button
            type="button"
            onClick={() => setTool("pen")}
            aria-label="Pen tool"
            className={`p-2 rounded-lg transition-colors ${
              tool === "pen" ? "bg-emerald-600 text-white" : "text-slate-400 hover:bg-slate-800 hover:text-white"
            }`}
          >
            <Pen className="w-4 h-4" />
          </button>
          <button
            type="button"
            onClick={() => setTool("eraser")}
            aria-label="Eraser tool"
            className={`p-2 rounded-lg transition-colors ${
              tool === "eraser" ? "bg-emerald-600 text-white" : "text-slate-400 hover:bg-slate-800 hover:text-white"
            }`}
          >
            <Eraser className="w-4 h-4" />
          </button>
          <button
            type="button"
            onClick={() => setTool("line")}
            aria-label="Line tool"
            className={`p-2 rounded-lg transition-colors ${
              tool === "line" ? "bg-emerald-600 text-white" : "text-slate-400 hover:bg-slate-800 hover:text-white"
            }`}
          >
            <Minus className="w-4 h-4" />
          </button>
          <button
            type="button"
            onClick={() => setTool("rect")}
            aria-label="Rectangle tool"
            className={`p-2 rounded-lg transition-colors ${
              tool === "rect" ? "bg-emerald-600 text-white" : "text-slate-400 hover:bg-slate-800 hover:text-white"
            }`}
          >
            <Square className="w-4 h-4" />
          </button>
          <button
            type="button"
            onClick={() => setTool("circle")}
            aria-label="Circle tool"
            className={`p-2 rounded-lg transition-colors ${
              tool === "circle" ? "bg-emerald-600 text-white" : "text-slate-400 hover:bg-slate-800 hover:text-white"
            }`}
          >
            <CircleIcon className="w-4 h-4" />
          </button>
          <button
            type="button"
            onClick={() => setTool("laser")}
            aria-label="Laser pointer"
            className={`p-2 rounded-lg transition-colors ${
              tool === "laser" ? "bg-red-600 text-white animate-pulse" : "text-slate-400 hover:bg-slate-800 hover:text-white"
            }`}
          >
            <Sparkles className="w-4 h-4" />
          </button>
        </div>

        {/* Color and Width */}
        <div className="flex items-center space-x-2">
          <input
            type="color"
            value={color}
            onChange={(e) => setColor(e.target.value)}
            aria-label="Stroke color"
            className="w-6 h-6 rounded border-0 cursor-pointer bg-transparent"
          />
          <input
            type="range"
            min={1}
            max={15}
            value={lineWidth}
            onChange={(e) => setLineWidth(Number(e.target.value))}
            aria-label="Stroke thickness"
            className="w-16 h-1 bg-slate-700 rounded-lg appearance-none cursor-pointer"
          />
        </div>

        {/* Multi-page Slide Controls */}
        <div className="flex items-center space-x-1.5 text-slate-300">
          <button
            type="button"
            disabled={currentPage <= 1}
            onClick={() => handlePageChange(currentPage - 1)}
            aria-label="Previous slide"
            className="p-1.5 rounded hover:bg-slate-800 disabled:opacity-30"
          >
            <ChevronLeft className="w-4 h-4" />
          </button>
          <span className="font-mono text-xs">
            {currentPage} / {totalPages}
          </span>
          <button
            type="button"
            onClick={() => handlePageChange(currentPage + 1)}
            aria-label="Next slide"
            className="p-1.5 rounded hover:bg-slate-800"
          >
            <ChevronRight className="w-4 h-4" />
          </button>
          {isHost && (
            <button
              type="button"
              onClick={() => handlePageChange(totalPages + 1)}
              aria-label="Add new slide"
              className="p-1.5 rounded hover:bg-slate-800 text-emerald-400"
            >
              <Plus className="w-4 h-4" />
            </button>
          )}
        </div>

        {/* Clear Action */}
        <button
          type="button"
          onClick={handleClearPage}
          aria-label="Clear current slide"
          className="p-2 rounded-lg text-slate-400 hover:bg-red-950/40 hover:text-red-400 transition-colors"
        >
          <Trash2 className="w-4 h-4" />
        </button>
      </div>

      {/* Canvas Area */}
      <div className="flex-1 relative bg-slate-950 overflow-hidden flex items-center justify-center">
        <canvas
          ref={canvasRef}
          width={1280}
          height={720}
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onMouseLeave={handleMouseUp}
          className="w-full h-full object-contain cursor-crosshair touch-none"
        />
      </div>
    </div>
  );
};
