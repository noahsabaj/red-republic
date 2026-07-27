// ============================================================
// The ONLY module that imports @tauri-apps/plugin-fs. Loaded lazily by
// the storage facade on desktop; the web bundle never fetches it.
// All paths are relative to the app-data directory
// (%APPDATA%\com.noahsabaj.redrepublic on Windows).
// ============================================================
import {
  BaseDirectory, exists, mkdir, readDir, readTextFile, remove, writeTextFile,
} from '@tauri-apps/plugin-fs';
import type { FsBackend } from './tauri-fs-driver';

const IN_APPDATA = { baseDir: BaseDirectory.AppData };

export function makeTauriFsBackend(): FsBackend {
  return {
    exists: p => exists(p, IN_APPDATA),
    mkdir: async p => {
      if (!(await exists(p, IN_APPDATA))) await mkdir(p, { ...IN_APPDATA, recursive: true });
    },
    readDir: async p => {
      const entries = await readDir(p, IN_APPDATA);
      return entries.filter(e => e.isFile).map(e => e.name);
    },
    readTextFile: p => readTextFile(p, IN_APPDATA),
    writeTextFile: (p, contents) => writeTextFile(p, contents, IN_APPDATA),
    remove: p => remove(p, IN_APPDATA),
  };
}
