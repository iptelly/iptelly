import { ScrollingModule } from "@angular/cdk/scrolling";
import { NgModule } from "@angular/core";
import { FormsModule } from "@angular/forms";
import { MatMenuModule } from "@angular/material/menu";
import { BrowserModule } from "@angular/platform-browser";
import { BrowserAnimationsModule } from "@angular/platform-browser/animations";
import { provideAnimationsAsync } from "@angular/platform-browser/animations/async";
import { NgbModalModule, NgbTooltipModule, NgbTypeaheadModule } from "@ng-bootstrap/ng-bootstrap";
import { ToastrModule } from "ngx-toastr";
import { AdultPinModalComponent } from "./adult-pin-modal/adult-pin-modal.component";
import { AppComponent } from "./app.component";
import { AppRoutingModule } from "./app-routing.module";
import { ChannelTileComponent } from "./channel-tile/channel-tile.component";
import { EpgTimelineComponent } from "./channel-tile/epg-timeline/epg-timeline.component";
import { ConfirmDeleteModalComponent } from "./confirm-delete-modal/confirm-delete-modal.component";
import { DeleteGroupModalComponent } from "./delete-group-modal/delete-group-modal.component";
import { DownloadManagerComponent } from "./download-manager/download-manager.component";
import { EditChannelModalComponent } from "./edit-channel-modal/edit-channel-modal.component";
import { EditGroupModalComponent } from "./edit-group-modal/edit-group-modal.component";
import { GroupNameExistsValidator } from "./edit-group-modal/validators/group-name-exists.directive";
import { EpgModalComponent } from "./epg-modal/epg-modal.component";
import { EpgModalItemComponent } from "./epg-modal/epg-modal-item/epg-modal-item.component";
import { ErrorModalComponent } from "./error-modal/error-modal.component";
import { DownloadSidebarComponent } from "./home/download-sidebar/download-sidebar.component";
import { EpgTimelineHeaderComponent } from "./home/epg-timeline-header/epg-timeline-header.component";
import { HomeComponent } from "./home/home.component";
import { NavRailComponent } from "./home/nav-rail/nav-rail.component";
import { PlaylistSidebarComponent } from "./home/playlist-sidebar/playlist-sidebar.component";
import { SortButtonComponent } from "./home/sort-button/sort-button.component";
import { SortItemComponent } from "./home/sort-button/sort-item/sort-item.component";
import { ImportModalComponent } from "./import-modal/import-modal.component";
import { LoadingComponent } from "./loading/loading.component";
import { ManageCategoriesComponent } from "./manage-categories/manage-categories.component";

import { TimeAgoPipe } from "./pipes/time-ago.pipe";
import { TimeUntilPipe } from "./pipes/time-until.pipe";
import { RestreamModalComponent } from "./restream-modal/restream-modal.component";
import { SettingsComponent } from "./settings/settings.component";
import { SourceTileComponent } from "./settings/source-tile/source-tile.component";
import { ConfirmModalComponent } from "./setup/confirm-modal/confirm-modal.component";
import { SetupComponent } from "./setup/setup.component";
import { NotEmptyValidatorDirective } from "./setup/validators/not-empty-validator.directive";
import { SourceNameExistsValidator } from "./setup/validators/source-name-exists-validator.directive";
import { ShortcutsHelpModalComponent } from "./shortcuts-help-modal/shortcuts-help-modal.component";

@NgModule({
  declarations: [
    AppComponent,
    TimeAgoPipe,
    TimeUntilPipe,
    SetupComponent,
    LoadingComponent,
    SourceNameExistsValidator,
    NotEmptyValidatorDirective,
    ConfirmModalComponent,
    HomeComponent,
    ChannelTileComponent,
    SettingsComponent,
    SourceTileComponent,
    ErrorModalComponent,
    EditChannelModalComponent,
    EditGroupModalComponent,
    GroupNameExistsValidator,
    DeleteGroupModalComponent,
    ImportModalComponent,
    ConfirmDeleteModalComponent,
    AdultPinModalComponent,
    EpgModalComponent,
    EpgModalItemComponent,
    RestreamModalComponent,
    SortButtonComponent,
    SortItemComponent,
    DownloadManagerComponent,
    NavRailComponent,
    PlaylistSidebarComponent,
    DownloadSidebarComponent,
    EpgTimelineComponent,
    EpgTimelineHeaderComponent,
    ManageCategoriesComponent,
    ShortcutsHelpModalComponent,
  ],
  imports: [
    BrowserModule,
    FormsModule,
    BrowserAnimationsModule,
    AppRoutingModule,
    NgbTooltipModule,
    ToastrModule.forRoot(),
    MatMenuModule,
    NgbModalModule,
    NgbTypeaheadModule,
    ScrollingModule,
  ],
  providers: [provideAnimationsAsync()],
  bootstrap: [AppComponent],
})
export class AppModule {}
