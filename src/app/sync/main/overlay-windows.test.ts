import { describe, expect, it, vi } from 'vitest';
import type { RendererCore } from '@app/roots/renderer-core';
import { syncOverlayWindows } from './overlay-windows';

const native = vi.hoisted(() => ({
  labels: new Set(['overlay-LEFT']),
  onDestroyed: vi.fn<() => void>(),
  close: vi.fn(async () => {}),
  unlisten: vi.fn(),
  created: vi.fn(),
}));

vi.mock('@shared/api/overlay-labels', () => ({
  monitorLabel: (name: string) => `overlay-${name}`,
  listOverlayWindowLabels: async () => [...native.labels],
}));

vi.mock('@tauri-apps/api/window', () => ({
  availableMonitors: async () => [
    {
      name: 'LEFT',
      position: { x: 0, y: 0 },
      size: { width: 1920, height: 1080 },
      scaleFactor: 1,
    },
  ],
}));

vi.mock('@tauri-apps/api/webviewWindow', () => ({
  WebviewWindow: class {
    static async getByLabel() {
      return {
        listen: async (event: string, notify: () => void) => {
          expect(event).toBe('tauri://destroyed');
          native.onDestroyed.mockImplementation(notify);

          return native.unlisten;
        },
        close: native.close,
        setPosition: vi.fn(),
        setSize: vi.fn(),
      };
    }

    constructor(label: string) {
      native.labels.add(label);
      native.created(label);
    }

    async once(event: string, notify: () => void) {
      if (event === 'tauri://created') {
        notify();
      }
    }

    setPosition = vi.fn();
    setSize = vi.fn();
    setIgnoreCursorEvents = vi.fn();
  },
}));

describe('syncOverlayWindows', () => {
  it('recreates an overlay reused by a layout switch during its pending close', async () => {
    const root = {
      appSettings: { dragMode: true },
      layouts: { liveMonitorNames: [] as string[] },
    };

    const closing = syncOverlayWindows(root as unknown as RendererCore);
    await vi.waitFor(() => expect(native.close).toHaveBeenCalledOnce());

    root.layouts.liveMonitorNames = ['LEFT'];
    const reopening = syncOverlayWindows(root as unknown as RendererCore);

    // Let the queued sync run if it is no longer waiting for native close.
    await new Promise<void>((resolve) => setTimeout(resolve, 0));

    native.labels.delete('overlay-LEFT');
    native.onDestroyed();
    await Promise.all([closing, reopening]);

    expect(native.unlisten).toHaveBeenCalledOnce();
    expect(native.created).toHaveBeenCalledExactlyOnceWith('overlay-LEFT');
    expect(native.labels.has('overlay-LEFT')).toBe(true);
  });
});
