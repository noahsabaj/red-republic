import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App.tsx'
import { initStorage } from './platform/storage'
import { initDesktop } from './platform/desktop'
import { reloadSettingsFromStorage } from './app/settings'

// Boot order matters: storage hydrates (desktop reads its save files),
// settings re-load from the now-active driver (their import-time load ran
// against provisional browser storage), the desktop shell wires its window
// handlers, and only then does the app render. In the browser every init
// resolves immediately — startup is unchanged.
void (async () => {
  await initStorage()
  reloadSettingsFromStorage()
  await initDesktop()
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  )
})()
