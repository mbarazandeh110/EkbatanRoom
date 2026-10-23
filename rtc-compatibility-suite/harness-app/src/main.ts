import { HarnessController } from './harness_api.js';

declare global {
  interface Window {
    testHarness: HarnessController;
  }
}

const harness = new HarnessController();
window.testHarness = harness;

console.info('[HarnessApp] WebRTC Compatibility Harness mounted on window.testHarness');
