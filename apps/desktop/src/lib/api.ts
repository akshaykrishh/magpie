import { invoke } from "@tauri-apps/api/core";
import type {
  AuditEntry,
  Blob,
  Capabilities,
  Capture,
  Project,
  ProjectFilter,
  ProjectOverview,
  Section,
  Session,
  Tag,
  Template,
} from "./types";

export const api = {
  listStream: (filter: ProjectFilter, limit = 200, offset = 0) =>
    invoke<Capture[]>("list_stream", { filter, limit, offset }),

  listNow: (projectId: number | null) =>
    invoke<Capture[]>("list_now", { projectId }),

  addTypedCapture: (body: string) => invoke<Capture>("add_typed_capture", { body }),

  promoteCapture: (id: number) => invoke<Capture>("promote_capture", { id }),
  demoteCapture: (id: number) => invoke<Capture>("demote_capture", { id }),

  reorderCapture: (id: number, afterId: number | null) =>
    invoke<Capture>("reorder_capture", { id, afterId }),

  markCaptureDone: (id: number) => invoke<Capture>("mark_capture_done", { id }),
  reopenCapture: (id: number) => invoke<Capture>("reopen_capture", { id }),

  searchCaptures: (query: string, limit = 50) =>
    invoke<Capture[]>("search_captures", { query, limit }),

  mergeCaptures: (ids: number[]) => invoke<Capture>("merge_captures", { ids }),

  listMergeSources: (mergedCaptureId: number) =>
    invoke<Capture[]>("list_merge_sources", { mergedCaptureId }),

  assignCaptureProject: (id: number, projectId: number | null) =>
    invoke<Capture>("assign_capture_project", { id, projectId }),

  addCaptureTag: (captureId: number, name: string) =>
    invoke<Tag>("add_capture_tag", { captureId, name }),

  removeCaptureTag: (captureId: number, name: string) =>
    invoke<void>("remove_capture_tag", { captureId, name }),

  listCaptureTags: (captureId: number) =>
    invoke<Tag[]>("list_capture_tags", { captureId }),

  listCapturesByTag: (name: string) =>
    invoke<Capture[]>("list_captures_by_tag", { name }),

  listProjects: () => invoke<Project[]>("list_projects"),

  getOrCreateProject: (
    name: string,
    remoteUrl: string | null,
    commonGitDir: string | null,
  ) =>
    invoke<Project>("get_or_create_project", {
      name,
      remoteUrl,
      commonGitDir,
    }),

  listSessions: (projectId: number | null) =>
    invoke<Session[]>("list_sessions", { projectId }),

  listProjectsOverview: () => invoke<ProjectOverview[]>("list_projects_overview"),

  exportJson: () => invoke<string>("export_json"),
  exportMarkdown: () => invoke<string>("export_markdown"),

  captureCapabilities: () => invoke<Capabilities>("capture_capabilities"),

  openAccessibilitySettings: () => invoke<void>("open_accessibility_settings"),

  listTemplates: () => invoke<Template[]>("list_templates"),

  createTemplate: (title: string, body: string) =>
    invoke<Template>("create_template", { title, body }),

  updateTemplate: (id: number, title: string, body: string) =>
    invoke<Template>("update_template", { id, title, body }),

  deleteTemplate: (id: number) => invoke<void>("delete_template", { id }),

  deleteCapture: (id: number) => invoke<Capture>("delete_capture", { id }),
  restoreCapture: (id: number) => invoke<Capture>("restore_capture", { id }),
  listRecentlyDeletedCaptures: () =>
    invoke<Capture[]>("list_recently_deleted_captures"),
  restoreTemplate: (id: number) => invoke<Template>("restore_template", { id }),
  listRecentlyDeletedTemplates: () =>
    invoke<Template[]>("list_recently_deleted_templates"),
  createSection: (name: string) => invoke<Section>("create_section", { name }),
  renameSection: (id: number, name: string) =>
    invoke<Section>("rename_section", { id, name }),
  listSections: () => invoke<Section[]>("list_sections"),
  reorderSection: (id: number, afterId: number | null) =>
    invoke<Section>("reorder_section", { id, afterId }),
  deleteSection: (id: number) => invoke<void>("delete_section", { id }),
  restoreSection: (id: number) => invoke<Section>("restore_section", { id }),
  assignCaptureSection: (id: number, sectionId: number | null) =>
    invoke<Capture>("assign_capture_section", { id, sectionId }),
  assignTemplateSection: (id: number, sectionId: number | null) =>
    invoke<Template>("assign_template_section", { id, sectionId }),

  instantiateTemplate: (templateId: number, projectId: number | null) =>
    invoke<Capture>("instantiate_template", { templateId, projectId }),

  instantiateTemplateIntoMany: (templateId: number, projectIds: (number | null)[]) =>
    invoke<Capture[]>("instantiate_template_into_many", { templateId, projectIds }),

  getTemplateVariables: (templateId: number) =>
    invoke<string[]>("get_template_variables", { templateId }),

  instantiateTemplateWithValues: (
    templateId: number,
    projectId: number | null,
    values: Record<string, string>,
  ) => invoke<Capture>("instantiate_template_with_values", { templateId, projectId, values }),

  listAudit: (limit = 50) => invoke<AuditEntry[]>("list_audit", { limit }),

  getCaptureBlob: (captureId: number) =>
    invoke<Blob | null>("get_capture_blob", { captureId }),

  getBlobImageDataUrl: (captureId: number) =>
    invoke<string | null>("get_blob_image_data_url", { captureId }),

  copyCaptureImage: (captureId: number) => invoke<void>("copy_capture_image", { captureId }),

  copyCaptureText: (id: number) => invoke<void>("copy_capture_text", { id }),

  copyCapturesAsChecklist: (ids: number[]) =>
    invoke<void>("copy_captures_as_checklist", { ids }),
};
