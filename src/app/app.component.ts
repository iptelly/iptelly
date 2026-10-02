import { Component, HostListener, OnInit } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { ThemeService } from "./theme.service";
import { Settings } from "./models/settings";

@Component({
  selector: "app-root",
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.css",
})
export class AppComponent implements OnInit {
  title = "iptelly";

  constructor(private theme: ThemeService) {}

  ngOnInit(): void {
    // Applied here (the true root, mounted before any routed page) rather
    // than in HomeComponent, so it's already correct even if Settings or
    // Manage Categories ends up being the first thing rendered.
    invoke("get_settings").then((settings) => {
      this.theme.applyTheme((settings as Settings).theme);
    });
  }

  @HostListener("document:contextmenu", ["$event"])
  onRightClick(event: MouseEvent) {
    const target = event.target as HTMLElement;
    if (this.isInsideMenuTrigger(target)) {
      return;
    }
    event.preventDefault();
  }

  private isInsideMenuTrigger(element: HTMLElement): boolean {
    return !!element.closest("[mat-menu-trigger-for], [matMenuTriggerFor]");
  }
}
