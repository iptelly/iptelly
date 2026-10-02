import { ScrollingModule } from "@angular/cdk/scrolling";
import { TestModuleMetadata } from "@angular/core/testing";
import { FormsModule } from "@angular/forms";
import { MatMenuModule } from "@angular/material/menu";
import { provideNoopAnimations } from "@angular/platform-browser/animations";
import { provideRouter, RouterModule } from "@angular/router";
import {
  NgbActiveModal,
  NgbModalModule,
  NgbTooltipModule,
  NgbTypeaheadModule,
} from "@ng-bootstrap/ng-bootstrap";
import { ToastrModule } from "ngx-toastr";
import { APP_DECLARATIONS } from "../app/app.module";

// The same declarations and template dependencies as AppModule, minus the
// browser bootstrapping and real routes, so any app component can be
// created in a test. NgbActiveModal is provided for the modal components,
// which normally get it from NgbModal.open().
export const appTestModule: TestModuleMetadata = {
  declarations: APP_DECLARATIONS,
  imports: [
    FormsModule,
    NgbTooltipModule,
    NgbModalModule,
    NgbTypeaheadModule,
    MatMenuModule,
    ScrollingModule,
    RouterModule,
    ToastrModule.forRoot(),
  ],
  providers: [provideRouter([]), provideNoopAnimations(), NgbActiveModal],
};
