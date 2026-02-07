import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import OutputView from "./OutputView";

function Router() {
  const params = new URLSearchParams(window.location.search);
  if (params.get("view") === "output") {
    return (
      <OutputView
        label={params.get("label") ?? ""}
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
