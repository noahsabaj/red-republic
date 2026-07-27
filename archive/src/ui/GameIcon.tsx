// The DOM half of the icon system — the registry lives in ./icons.ts.
import { createElement } from 'react';
import { GAME_ICONS, type GameIconName } from './icons';

export function GameIcon({ name, size = 13, className = '' }: { name: string; size?: number; className?: string }) {
  const node = GAME_ICONS[name as GameIconName];
  if (!node) return null;
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={`inline-block align-[-0.15em] shrink-0 ${className}`}
    >
      {node.map(([tag, attrs], i) => createElement(tag, { ...attrs, key: i }))}
    </svg>
  );
}
