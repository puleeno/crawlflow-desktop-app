import { describe, it, expect, vi } from 'vitest';
import React from 'react';

vi.mock('../components/icons', () => ({
  PlayIcon: () => React.createElement('span', { 'data-testid': 'play-icon' }, '▶'),
  StopIcon: () => React.createElement('span', { 'data-testid': 'stop-icon' }, '■'),
  PauseIcon: () => React.createElement('span', { 'data-testid': 'pause-icon' }, '⏸'),
  XMarkIcon: () => React.createElement('span', { 'data-testid': 'xmark-icon' }, '✕'),
  ChevronDownIcon: () => React.createElement('span', { 'data-testid': 'chevron-down' }, '▼'),
  ChevronUpIcon: () => React.createElement('span', { 'data-testid': 'chevron-up' }, '▲'),
}));

describe('LiveLogs component', () => {
  it('should import without crashing', async () => {
    const LiveLogs = (await import('../components/LiveLogs')).default;
    expect(LiveLogs).toBeDefined();
  });
});
