import { useEffect, useRef, useState } from 'react';

export interface Toast { id: number; text: string; kind: 'good' | 'bad' | 'info' }

export function useToasts() {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const timers = useRef<Set<ReturnType<typeof setTimeout>>>(new Set());
  const nextId = useRef(1);

  useEffect(() => {
    const pending = timers.current;
    return () => { for (const t of pending) clearTimeout(t); };
  }, []);

  const push = (text: string, kind: Toast['kind'] = 'info') => {
    const id = nextId.current++;
    setToasts(ts => [...ts.slice(-4), { id, text, kind }]);
    const timer = setTimeout(() => {
      timers.current.delete(timer);
      setToasts(ts => ts.filter(t => t.id !== id));
    }, 4200);
    timers.current.add(timer);
  };
  return { toasts, push };
}
