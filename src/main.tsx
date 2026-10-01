import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { text } from "./app/texts";
import "./styles/tokens.css";
import "./styles/global.css";

const rootElement = document.querySelector<HTMLElement>("#root");

if (rootElement === null) {
  throw new Error(text.rootElementMissing);
}

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
