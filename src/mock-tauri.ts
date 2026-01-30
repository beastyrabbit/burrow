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

  if (query.startsWith(":")) {
    const cmd = query.slice(1).trim().toLowerCase();
    const settings = [
      { id: "reindex", name: ":reindex", description: "Reindex all files (full rebuild)" },
      { id: "update", name: ":update", description: "Update index (incremental)" },
      { id: "config", name: ":config", description: "Open config file" },
      { id: "stats", name: ":stats", description: "Index statistics" },
      { id: "progress", name: ":progress", description: "Show indexer progress" },
    ];
    return settings
      .filter((s) => !cmd || s.id.includes(cmd) || s.name.includes(cmd) || s.description.toLowerCase().includes(cmd))
      .map((s) => ({
        id: s.id,
        name: s.name,
        description: s.description,
        icon: "",
        category: "action",
        exec: "",
      }));
  }

  if (query.startsWith(" ")) {
    const q = query.trimStart();
    if (q.startsWith("*")) {
      const contentQuery = q.slice(1).trim();
      if (!contentQuery) return [];
      return [
        {
          id: "/home/user/docs/rust-guide.md",
          name: "rust-guide.md",
          description: `87% — A guide to ${contentQuery}`,
          icon: "",
          category: "vector",
          exec: "xdg-open /home/user/docs/rust-guide.md",
        },
        {
          id: "/home/user/notes/setup.txt",
          name: "setup.txt",
          description: `62% — Notes about ${contentQuery}`,
          icon: "",
          category: "vector",
          exec: "xdg-open /home/user/notes/setup.txt",
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

function mockRunSetting(args: Record<string, unknown>): string {
  const action = args.action as string;
  switch (action) {
    case "reindex":
      return "Reindexing started in background...";
    case "update":
      return "Incremental update started in background...";
    case "config":
      return "Opened ~/.config/burrow/config.toml";
    case "stats":
      return "Content indexed: 0 files | Apps tracked: 0 launches | Last indexed: never";
    case "progress":
      return "Idle | No indexing has run yet";
    default:
      throw new Error(`Unknown setting action: ${action}`);
  }
}

const handlers: Record<string, MockHandler> = {
  search: mockSearch,
  record_launch: () => null,
  launch_app: () => null,
  run_setting: mockRunSetting,
};

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const handler = handlers[cmd];
  if (!handler) {
    throw new Error(`Unknown mock command: ${cmd}`);
  }
  return handler(args || {}) as T;
}
