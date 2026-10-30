import React from "react";
import ReactDOM from "react-dom/client";
import { MainApp } from "./main/MainApp";
import { OverlayApp } from "./overlay/OverlayApp";
import { windowView } from "./tauri";
import "./styles/theme.css";
import "./styles/base.css";
import "./styles/overlay.css";

// Both windows load this page. The view decides which one to draw, and the stylesheets
// key off `data-view` so the overlay stays transparent while the main window is solid.
const view = windowView();
document.documentElement.dataset.view = view;
document.documentElement.dataset.theme = "dark";
document.documentElement.dataset.platform = /Mac/i.test(navigator.userAgent) ? "mac" : "other";
const Root = view === "overlay" ? OverlayApp : MainApp;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
