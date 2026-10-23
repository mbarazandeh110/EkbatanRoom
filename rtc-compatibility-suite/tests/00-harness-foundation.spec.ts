import { test, expect, sharedEvidenceRecorder } from './fixtures/test_context.js';
import * as fs from 'fs';
import * as path from 'path';

test.describe.configure({ mode: 'serial' });

test.describe('Slice 1 — Harness Foundation & Evidence Infrastructure', () => {
  test('verifies harness DOM and window.testHarness initialization', async ({ createParticipant, tokenFactory }) => {
    const participant = await createParticipant('harness-verifier', 'foundation-room');
    const page = participant.getPage();

    // Verify DOM structure
    await expect(page.locator('#harness-status')).toHaveText('IDLE');
    await expect(page.locator('#local-video')).toBeVisible();
    await expect(page.locator('#remote-video')).toBeVisible();

    // Verify window.testHarness presence
    const harnessState = await page.evaluate(() => {
      return window.testHarness.getState();
    });

    expect(harnessState.status).toBe('IDLE');
    expect(harnessState.connectionState).toBe('disconnected');

    // Verify TokenFactory creates a non-empty credential
    const token = await tokenFactory.createToken('foundation-room', 'harness-verifier');
    expect(token).toBeDefined();
    expect(token.length).toBeGreaterThan(20);

    // Save evidence report to disk
    const reportPath = path.resolve(process.cwd(), 'test-results/evidence-report.json');
    const report = sharedEvidenceRecorder.saveReport(reportPath);

    expect(fs.existsSync(reportPath)).toBe(true);
    expect(report.sdkVersion).toBe('2.23.0');
    expect(report.browser.name).toBe('chromium');

    // Strict Slice 1 gate rule: Gates 1 through 5 must NOT be marked PASSED
    expect(report.gates.gate_01_connection.status).toBe('NOT_RUN');
    expect(report.gates.gate_02_bidirectional_rtp.status).toBe('NOT_RUN');
    expect(report.gates.gate_03_concurrency_glare.status).toBe('NOT_RUN');
    expect(report.gates.gate_04_simulcast_layers.status).toBe('NOT_RUN');
    expect(report.gates.gate_05_ice_restart.status).toBe('NOT_RUN');
  });
});
