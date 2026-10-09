import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@fontsource-variable/inter'
import '@fontsource-variable/caveat'
// Text fonts on the board (see FONT_STACK); the browser fetches each one only once it is used.
import '@fontsource-variable/nunito'
import '@fontsource-variable/montserrat'
import '@fontsource-variable/oswald'
import '@fontsource-variable/lora'
import '@fontsource-variable/playfair-display'
import '@fontsource/patrick-hand'
import '@fontsource/permanent-marker'
import './styles.css'
import { App } from './App.tsx'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
