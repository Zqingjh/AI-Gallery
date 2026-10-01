import { text } from "../app/texts";
import { useTheme } from "../app/theme";

export function ThemeToggle() {
  const { theme, toggleTheme } = useTheme();
  const label = theme === "dark" ? text.theme.light : text.theme.dark;

  return (
    <button
      className="icon-button"
      type="button"
      onClick={toggleTheme}
      aria-label={label}
    >
      <span aria-hidden="true">{theme === "dark" ? "☼" : "◐"}</span>
    </button>
  );
}
