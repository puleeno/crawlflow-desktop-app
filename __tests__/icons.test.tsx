import { describe, it, expect } from 'vitest';
import React from 'react';
import { render } from '@testing-library/react';
import {
  PlayIcon, StopIcon, PauseIcon, Cog6ToothIcon, XMarkIcon,
  HomeIcon, PlusIcon, TrashIcon, SearchIcon, FolderIcon,
  DatabaseIcon, GlobeAltIcon, CloudIcon, DocumentTextIcon,
  CursorArrowRaysIcon, ArrowPathIcon, DocumentMagnifyingGlassIcon,
  Bars3Icon, ArrowUpTrayIcon, ArrowDownTrayIcon, ChevronDownIcon,
  ChevronUpIcon, DocumentDuplicateIcon, HandIcon, CpuChipIcon,
  TableCellsIcon, FlagIcon, SquareIcon, CircleIcon, EllipseIcon,
  FrameIcon, ArchiveBoxIcon, FunnelIcon,
} from '../components/icons';

const iconComponents = [
  { name: 'PlayIcon', Comp: PlayIcon },
  { name: 'StopIcon', Comp: StopIcon },
  { name: 'PauseIcon', Comp: PauseIcon },
  { name: 'Cog6ToothIcon', Comp: Cog6ToothIcon },
  { name: 'XMarkIcon', Comp: XMarkIcon },
  { name: 'HomeIcon', Comp: HomeIcon },
  { name: 'PlusIcon', Comp: PlusIcon },
  { name: 'TrashIcon', Comp: TrashIcon },
  { name: 'SearchIcon', Comp: SearchIcon },
  { name: 'FolderIcon', Comp: FolderIcon },
  { name: 'DatabaseIcon', Comp: DatabaseIcon },
  { name: 'GlobeAltIcon', Comp: GlobeAltIcon },
  { name: 'CloudIcon', Comp: CloudIcon },
  { name: 'DocumentTextIcon', Comp: DocumentTextIcon },
  { name: 'CursorArrowRaysIcon', Comp: CursorArrowRaysIcon },
  { name: 'ArrowPathIcon', Comp: ArrowPathIcon },
  { name: 'DocumentMagnifyingGlassIcon', Comp: DocumentMagnifyingGlassIcon },
  { name: 'Bars3Icon', Comp: Bars3Icon },
  { name: 'ArrowUpTrayIcon', Comp: ArrowUpTrayIcon },
  { name: 'ArrowDownTrayIcon', Comp: ArrowDownTrayIcon },
  { name: 'ChevronDownIcon', Comp: ChevronDownIcon },
  { name: 'ChevronUpIcon', Comp: ChevronUpIcon },
  { name: 'DocumentDuplicateIcon', Comp: DocumentDuplicateIcon },
  { name: 'HandIcon', Comp: HandIcon },
  { name: 'CpuChipIcon', Comp: CpuChipIcon },
  { name: 'TableCellsIcon', Comp: TableCellsIcon },
  { name: 'FlagIcon', Comp: FlagIcon },
  { name: 'SquareIcon', Comp: SquareIcon },
  { name: 'CircleIcon', Comp: CircleIcon },
  { name: 'EllipseIcon', Comp: EllipseIcon },
  { name: 'FrameIcon', Comp: FrameIcon },
  { name: 'ArchiveBoxIcon', Comp: ArchiveBoxIcon },
  { name: 'FunnelIcon', Comp: FunnelIcon },
];

describe('Icons', () => {
  it.each(iconComponents)('$name should render an SVG element', ({ Comp }) => {
    const { container } = render(React.createElement(Comp));
    const svg = container.querySelector('svg');
    expect(svg).not.toBeNull();
    expect(svg!.getAttribute('xmlns')).toBe('http://www.w3.org/2000/svg');
  });

  it('PlayIcon should accept size prop', () => {
    const { container } = render(<PlayIcon size={32} />);
    const svg = container.querySelector('svg');
    expect(svg!.getAttribute('width')).toBe('32');
    expect(svg!.getAttribute('height')).toBe('32');
  });

  it('StopIcon should accept className prop', () => {
    const { container } = render(<StopIcon className="custom-class" />);
    const svg = container.querySelector('svg');
    expect(svg!.getAttribute('class')).toContain('custom-class');
  });

  it('PauseIcon should render with default size', () => {
    const { container } = render(<PauseIcon />);
    const svg = container.querySelector('svg');
    expect(svg!.getAttribute('width')).toBe('20');
  });

  it('HomeIcon should render with default size 24', () => {
    const { container } = render(<HomeIcon />);
    const svg = container.querySelector('svg');
    expect(svg!.getAttribute('width')).toBe('24');
  });

  it('PlusIcon should render with default size 20', () => {
    const { container } = render(<PlusIcon />);
    const svg = container.querySelector('svg');
    expect(svg!.getAttribute('width')).toBe('20');
  });

  it('all icons should have a viewBox attribute', () => {
    for (const { Comp } of iconComponents) {
      const { container } = render(React.createElement(Comp));
      const svg = container.querySelector('svg');
      expect(svg!.getAttribute('viewBox')).not.toBeNull();
    }
  });

  it('all icons should have a path or circle/ellipse element', () => {
    for (const { name, Comp } of iconComponents) {
      const { container } = render(React.createElement(Comp));
      const hasShape = container.querySelector('path, circle, ellipse');
      expect(hasShape, `${name} is missing shape element`).not.toBeNull();
    }
  });
});
