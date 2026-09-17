import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { hasWindowControls } from './lib/desktop'
import './index.css'

const container = document.getElementById('root')
if (!container) throw new Error('Root element missing from index.html')

// Set before the first render so the app never paints under the titlebar and
// then jumps. Only when the window controls actually work: a 36px gap with no
// titlebar in it would be worse than no titlebar at all.
if (hasWindowControls()) document.documentElement.classList.add('desktop')

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
