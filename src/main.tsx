import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import OutputView from "./OutputView";
import CodexOutputView from "./CodexOutputView";

function Router() {
  const params = new URLSearchParams(window.location.search);
  if (params.get("view") === "output") {
    const label = params.get("label");
    if (!label) {
      return <div style={{ color: "#f44", padding: 24 }}>Error: missing &quot;label&quot; parameter</div>;
    }
    const title = params.get("title") ?? "Output";
    const format = params.get("format");
    if (format === "codex_json") {
      return <CodexOutputView label={label} title={title} />;
    }
    return <OutputView label={label} title={title} />;
  }
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Router />
  </React.StrictMode>,
);
