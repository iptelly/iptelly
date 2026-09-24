import { Injectable } from "@angular/core";

@Injectable({
  providedIn: "root",
})
export class ThemeService {
  // Always sets an explicit value rather than ever relying on the
  // attribute's absence to mean "modern" - styles.css has a matching
  // explicit :root[data-theme="modern"] block (as well as plain :root, for
  // the very first paint before this runs), so there's no cascade/timing
  // subtlety to get backwards.
  applyTheme(theme: string | undefined) {
    document.documentElement.setAttribute("data-theme", theme === "classic" ? "classic" : "modern");
  }
}
