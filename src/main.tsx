import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource/inter";
import "./styles/global.css";
import App from "./App";
import { initEventListeners } from "./store";

initEventListeners();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
