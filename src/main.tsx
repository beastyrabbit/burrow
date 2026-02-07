import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import OutputView from "./OutputView";

function Router() {
  const params = new URLSearchParams(window.location.search);
  if (params.get("view") === "output") {
    const label = params.get("label");
    if (!label) {
      return <div style={{ color: "#f44", padding: 24 }}>Error: missing &quot;label&quot; parameter</div>;
    }
    return (
      <OutputView
        label={label}
        title={params.get("title") ?? "Output"}
      />
    );
  }
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Router />
  </React.StrictMode>,
);
