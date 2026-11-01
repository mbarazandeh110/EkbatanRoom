import type { Page, WebSocket } from '@playwright/test';
import type { IParticipantController, JoinOptions } from './interfaces.js';
import { StatsCollector, type RawStatsReportEntry } from './stats_collector.js';
import type { RtpMetricsSnapshot } from '../domain/qoe_metrics.js';

export class ParticipantClientController implements IParticipantController {
  private page: Page;
  private identity: string;
  private roomId: string;

  private activeWebSockets: WebSocket[] = [];
  private wsListeners: Array<(direction: 'inbound' | 'outbound', payload: string | Buffer) => void> = [];

  constructor(page: Page, identity: string, roomId = 'default-room') {
    this.page = page;
    this.identity = identity;
    this.roomId = roomId;

    this.page.on('websocket', (ws: WebSocket) => {
      this.activeWebSockets.push(ws);
      ws.on('framesent', (event) => {
        for (const listener of this.wsListeners) {
          try {
            listener('outbound', event.payload);
          } catch (e) {
            console.error('Error in wsListener outbound:', e);
          }
        }
      });
      ws.on('framereceived', (event) => {
        for (const listener of this.wsListeners) {
          try {
            listener('inbound', event.payload);
          } catch (e) {
            console.error('Error in wsListener inbound:', e);
          }
        }
      });
    });
  }

  public getIdentity(): string {
    return this.identity;
  }

  public getRoomId(): string {
    return this.roomId;
  }

  public getPage(): Page {
    return this.page;
  }

  public async navigate(harnessUrl: string): Promise<void> {
    await this.page.goto(harnessUrl, { waitUntil: 'domcontentloaded' });
    await this.page.waitForFunction(() => !!window.testHarness, { timeout: 10000 });
  }

  public async join(url: string, token: string, options?: JoinOptions): Promise<void> {
    // Pass query parameters to assist signaling-coordinator when query params are used
    let connectUrl = url;
    if (!connectUrl.includes('?')) {
      connectUrl = `${url}?room=${encodeURIComponent(this.roomId)}&identity=${encodeURIComponent(this.identity)}`;
    }

    await this.page.evaluate(
      async ({ targetUrl, jwtToken, opts }) => {
        await window.testHarness.connect(targetUrl, jwtToken, {
          autoSubscribe: opts?.autoSubscribe ?? true
        });
      },
      { targetUrl: connectUrl, jwtToken: token, opts: options }
    );

    // Wait until harness state becomes CONNECTED
    await this.page.waitForFunction(
      () => window.testHarness.getState().status === 'CONNECTED',
      { timeout: 15000 }
    );
  }

  public async publishTracks(options: {
    audio?: boolean;
    video?: boolean;
    simulcast?: boolean;
  }): Promise<void> {
    await this.page.evaluate(async (opts) => {
      await window.testHarness.publishTracks(opts);
    }, options);
  }

  public async setMicrophoneEnabled(enabled: boolean): Promise<void> {
    await this.page.evaluate(async (en) => {
      await window.testHarness.setMicrophoneEnabled(en);
    }, enabled);
  }

  public async setCameraEnabled(enabled: boolean): Promise<void> {
    await this.page.evaluate(async (en) => {
      await window.testHarness.setCameraEnabled(en);
    }, enabled);
  }

  public async setVideoQuality(quality: 'low' | 'medium' | 'high'): Promise<void> {
    await this.page.evaluate(async (q) => {
      await window.testHarness.setVideoQuality(q);
    }, quality);
  }

  public async restartIce(): Promise<void> {
    await this.page.evaluate(async () => {
      await window.testHarness.restartIce();
    });
  }

  public async getStats(): Promise<RtpMetricsSnapshot> {
    const rawReports = await this.page.evaluate(async () => {
      const stats = await window.testHarness.getPeerConnectionStats();
      const state = window.testHarness.getState();
      return { stats, state };
    });

    return StatsCollector.parseStatsReport(
      rawReports.stats as unknown as RawStatsReportEntry[],
      'connected',
      rawReports.state.connectionState,
      'stable'
    );
  }

  public async getRenderedVideoStats(selector = '#remote-video'): Promise<{
    videoWidth: number;
    videoHeight: number;
    readyState: number;
    currentTime: number;
  }> {
    return this.page.evaluate((sel) => {
      return window.testHarness.getRenderedVideoStats(sel);
    }, selector);
  }

  public async waitForRemoteParticipant(participantIdentity?: string, timeoutMs = 15000): Promise<void> {
    await this.page.waitForFunction(
      (expectedId) => {
        const state = window.testHarness.getState();
        if (expectedId) {
          return state.remoteParticipants.includes(expectedId);
        }
        return state.remoteParticipants.length > 0;
      },
      participantIdentity,
      { timeout: timeoutMs }
    );
  }

  public async waitForRemoteParticipantCount(expectedCount: number, timeoutMs = 15000): Promise<void> {
    await this.page.waitForFunction(
      (count) => {
        const state = window.testHarness.getState();
        return state.remoteParticipants.length === count;
      },
      expectedCount,
      { timeout: timeoutMs }
    );
  }

  public async getIceConnectionState(): Promise<string> {
    return this.page.evaluate(() => {
      const room = (window.testHarness as any)?.room;
      const pc = room?.engine?.pcManager?.publisher?.pc ?? room?.engine?.pcManager?.publisher;
      return pc?.iceConnectionState || 'unknown';
    });
  }

  public async waitForRemoteTrack(kind: 'audio' | 'video', timeoutMs = 15000): Promise<void> {
    const selector = kind === 'video' ? '#remote-video' : '#remote-audio';
    await this.page.waitForFunction(
      (sel) => {
        const el = document.querySelector(sel) as HTMLMediaElement | null;
        if (!el) return false;
        if (el instanceof HTMLVideoElement) {
          return el.videoWidth > 0 && el.readyState >= 2;
        }
        return el.readyState >= 2;
      },
      selector,
      { timeout: timeoutMs }
    );
  }

  public async leave(): Promise<void> {
    await this.page.evaluate(async () => {
      await window.testHarness.disconnect();
    });
  }

  public attachWebSocketListener(
    onFrame: (direction: 'inbound' | 'outbound', payload: string | Buffer) => void
  ): () => void {
    this.wsListeners.push(onFrame);
    return () => {
      const idx = this.wsListeners.indexOf(onFrame);
      if (idx !== -1) {
        this.wsListeners.splice(idx, 1);
      }
    };
  }

  public attachConsoleListener(onLog: (line: string) => void): void {
    this.page.on('console', (msg) => {
      onLog(`[console.${msg.type()}] ${msg.text()}`);
    });
    this.page.on('pageerror', (err) => {
      onLog(`[pageerror] ${err.message}`);
    });
  }
}
