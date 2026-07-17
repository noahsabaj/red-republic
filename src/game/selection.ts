// ============================================================
// Selection model — what the player has clicked on the map.
// Pure data + pure update logic so it is unit-testable.
// ============================================================

export type SelectionItem =
  | { kind: 'building'; id: number }
  | { kind: 'deposit'; x: number; y: number };

export function sameItem(a: SelectionItem, b: SelectionItem): boolean {
  if (a.kind === 'building') return b.kind === 'building' && a.id === b.id;
  return b.kind === 'deposit' && a.x === b.x && a.y === b.y;
}

/**
 * Click semantics:
 *  - plain click: replace the selection (empty ground clears it)
 *  - shift/ctrl click (additive): toggle the item; empty ground keeps
 *    the current selection
 */
export function updateSelection(current: SelectionItem[], item: SelectionItem | null, additive: boolean): SelectionItem[] {
  if (!additive) return item ? [item] : [];
  if (!item) return current;
  const without = current.filter(i => !sameItem(i, item));
  return without.length < current.length ? without : [...current, item];
}
