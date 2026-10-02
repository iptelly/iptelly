import { Component, Input } from "@angular/core";
import { MemoryService } from "../../../memory.service";
import { getSortTypeText, SortType } from "../../../models/sortType";

@Component({
  selector: "app-sort-item",
  templateUrl: "./sort-item.component.html",
  styleUrl: "./sort-item.component.css",
})
export class SortItemComponent {
  constructor(public memory: MemoryService) {}

  @Input()
  sortType?: SortType;

  getText(): string {
    return getSortTypeText(this.sortType);
  }

  notifySortChange() {
    this.memory.Sort.next([this.sortType!, true]);
  }
}
