export function buildPlaywrightUse(frontendUrl) {
  return {
    baseURL: frontendUrl,
    headless: true,
  };
}

export function buildFrontendWebServer(frontendUrl) {
  return {
    command: "pnpm dev",
    url: frontendUrl,
    reuseExistingServer: true,
    timeout: 10000,
  };
}
