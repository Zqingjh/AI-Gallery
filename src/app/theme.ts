import { useCallback, useEffect, useState } from "react";

export type Theme = "dark" | "light";

const themeStorageKey = "ai-gallery-theme";

function getInitialTheme(): Theme {
  try {
    const savedTheme = window.localStorage.getItem(themeStorageKey);
    return savedTheme === "light" || savedTheme === "dark"
      ? savedTheme
      : "dark";
  } catch {
    // 隐私模式或受限 WebView 可能禁用存储，主题仍应可靠启动。
    return "dark";
  }
}

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(getInitialTheme);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      window.localStorage.setItem(themeStorageKey, theme);
    } catch {
      // 主题切换不应因偏好无法持久化而中断界面。
    }
  }, [theme]);

  const toggleTheme = useCallback(() => {
    setTheme((currentTheme) => (currentTheme === "dark" ? "light" : "dark"));
  }, []);

  return { theme, toggleTheme } as const;
}
