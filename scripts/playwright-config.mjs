function usesHttps(frontendUrl) {
  return frontendUrl.startsWith("https://");
}

export function buildPlaywrightUse(frontendUrl) {
  return {
    baseURL: frontendUrl,
    headless: true,
    ...(usesHttps(frontendUrl) ? { ignoreHTTPSErrors: true } : {}),
  };
}

export function buildFrontendWebServer(frontendUrl) {
  return {
    command: "pnpm dev",
    url: frontendUrl,
    reuseExistingServer: true,
    timeout: 10000,
    ...(usesHttps(frontendUrl) ? { ignoreHTTPSErrors: true } : {}),
  };
}
