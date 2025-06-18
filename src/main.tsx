import React from "react";
import ReactDOM from "react-dom/client";
import 'katex/dist/katex.min.css';   // 全局一次性加载
import App from "./App";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
