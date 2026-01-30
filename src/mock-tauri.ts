// Mock Tauri invoke for browser-only testing (Playwright).
// Simulates backend responses for each command.

interface SearchResult {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  exec: string;
}

type MockHandler = (args: Record<string, unknown>) => unknown;

const MATH_RE = /[+\-*/^%()]/;

function mockSearch(args: Record<string, unknown>): SearchResult[] {
  const query = (args.query as string) || "";

  if (query === "") {
    return [
      {
        id: "firefox",
        name: "Firefox",
        description: "Web Browser",
        icon: "firefox",
        category: "history",
        exec: "firefox",
      },
      {
        id: "kitty",
        name: "Kitty",
        description: "Terminal Emulator",
        icon: "kitty",
        category: "history",
        exec: "kitty",
      },
    ];
  }

  if (query.startsWith(" ")) {
    const q = query.trimStart();
    if (q.startsWith("*")) {
      return [
        {
          id: "vector-placeholder",
          name: "Content search not yet available",
          description: "Ollama integration pending",
          icon: "",
          category: "info",
          exec: "",
        },
      ];
    }
    // File search mock
    if (q) {
      return [
        {
          id: `/home/user/Documents/${q}.txt`,
          name: `${q}.txt`,
          description: "/home/user/Documents",
          icon: "",
          category: "file",
          exec: `xdg-open /home/user/Documents/${q}.txt`,
        },
      ];
    }
    return [];
  }

  if (query.startsWith("!")) {
    const q = query.slice(1).trim();
    if (!q) return [];
    return [
      {
        id: `op-${q}`,
        name: q,
        description: "Login",
        icon: "",
        category: "onepass",
        exec: `op item get ${q}`,
      },
    ];
  }

  if (query.startsWith("ssh ") || query === "ssh") {
    const q = query.replace(/^ssh\s*/, "");
    return [
      {
        id: "ssh-devbox",
        name: "devbox",
        description: "admin@10.0.0.5",
        icon: "",
        category: "ssh",
        exec: "kitty ssh admin@devbox",
      },
    ].filter((h) => !q || h.name.includes(q));
  }

  // Math detection
  if (MATH_RE.test(query)) {
    try {
      // Simple math for mock purposes
      const result = Function(`"use strict"; return (${query})`)();
      if (typeof result === "number" && isFinite(result)) {
        return [
          {
            id: "math-result",
            name: `= ${result}`,
            description: `${query} = ${result}`,
            icon: "",
            category: "math",
            exec: "",
          },
        ];
      }
    } catch {
      // Not valid math
    }
  }

  // App search mock
  const apps = [
    { id: "firefox", name: "Firefox", desc: "Web Browser", exec: "firefox" },
    {
      id: "chromium",
      name: "Chromium",
      desc: "Web Browser",
      exec: "chromium",
    },
    { id: "kitty", name: "Kitty", desc: "Terminal", exec: "kitty" },
    { id: "code", name: "VS Code", desc: "Code Editor", exec: "code" },
    {
      id: "nautilus",
      name: "Files",
      desc: "File Manager",
      exec: "nautilus",
    },
  ];

  const q = query.toLowerCase();
  return apps
    .filter((a) => a.name.toLowerCase().includes(q))
    .map((a) => ({
      id: a.id,
      name: a.name,
      description: a.desc,
      icon: a.id,
      category: "app",
      exec: a.exec,
    }));
}

const handlers: Record<string, MockHandler> = {
  search: mockSearch,
  record_launch: () => null,
  launch_app: () => null,
};

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const handler = handlers[cmd];
  if (!handler) {
    throw new Error(`Unknown mock command: ${cmd}`);
  }
  return handler(args || {}) as T;
}
