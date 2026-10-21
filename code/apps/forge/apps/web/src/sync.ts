import { HttpTransport, IndexedDbStore, SyncClient } from "@code-villain/sync-client";
import type {
  AiRun,
  Comment,
  CommentCreate,
  CommentPatch,
  Decision,
  DecisionCreate,
  DecisionPatch,
  Domain,
  DomainCreate,
  DomainPatch,
  Initiative,
  InitiativeCreate,
  InitiativePatch,
  Issue,
  IssueCreate,
  IssueLink,
  IssuePatch,
  IssueRelation,
  IssueRelationCreate,
  IssueRelationPatch,
  Label,
  LabelCreate,
  LabelPatch,
  Notification,
  NotificationPatch,
  Project,
  ProjectCreate,
  ProjectPatch,
  RequirementVerdict,
  Service,
  ServiceCreate,
  ServicePatch,
  Spec,
  SpecChange,
  SpecCreate,
  SpecPatch,
  Suggestion,
  Team,
  TeamCreate,
  TeamPatch,
  User,
  WorkflowState,
  WorkflowStateCreate,
  WorkflowStatePatch,
} from "@forge/types";
import { authHeaders, credentialsFor, type Place, socketProtocols } from "./server";

export type Schema = {
  Team: { record: Team; create: TeamCreate; patch: TeamPatch };
  WorkflowState: { record: WorkflowState; create: WorkflowStateCreate; patch: WorkflowStatePatch };
  Label: { record: Label; create: LabelCreate; patch: LabelPatch };
  Issue: { record: Issue; create: IssueCreate; patch: IssuePatch };
  IssueRelation: { record: IssueRelation; create: IssueRelationCreate; patch: IssueRelationPatch };
  Comment: { record: Comment; create: CommentCreate; patch: CommentPatch };
  Project: { record: Project; create: ProjectCreate; patch: ProjectPatch };
  Initiative: { record: Initiative; create: InitiativeCreate; patch: InitiativePatch };
  User: { record: User; create: never; patch: never };
  Notification: { record: Notification; create: never; patch: NotificationPatch };
  Service: { record: Service; create: ServiceCreate; patch: ServicePatch };
  Domain: { record: Domain; create: DomainCreate; patch: DomainPatch };
  Spec: { record: Spec; create: SpecCreate; patch: SpecPatch };
  Decision: { record: Decision; create: DecisionCreate; patch: DecisionPatch };
  AiRun: { record: AiRun; create: never; patch: never };
  Suggestion: { record: Suggestion; create: never; patch: never };
  IssueLink: { record: IssueLink; create: never; patch: never };
  RequirementVerdict: { record: RequirementVerdict; create: never; patch: never };
  SpecChange: { record: SpecChange; create: never; patch: never };
};

/** Each person's copy of each workspace lives in its own database. */
export function databaseName(userId: string, workspaceId: string) {
  return `forge-${userId}-${workspaceId}`;
}

const timestamps = () => {
  const now = new Date().toISOString();
  return { created_at: now, updated_at: now };
};

export function createSyncClient(options: {
  place: Place;
  userId: string;
  workspaceId: string;
  onSessionEnded: () => void;
  onRefused: (message: string) => void;
}) {
  return new SyncClient<Schema>({
    store: new IndexedDbStore(databaseName(options.userId, options.workspaceId)),
    transport: new HttpTransport({
      workspaceId: options.workspaceId,
      baseUrl: options.place.origin,
      onUnauthorized: options.onSessionEnded,
      credentials: credentialsFor(options.place),
      headers: () => authHeaders(options.place),
      protocols: () => socketProtocols(options.place),
    }),
    // What the server would fill in, so new records look complete at once.
    defaults: {
      Issue: () => ({
        number: 0,
        description: "",
        priority: 0,
        assignee_id: null,
        parent_id: null,
        project_id: null,
        service_id: null,
        label_ids: [],
        due_date: null,
        sort_order: 0,
        completed_at: null,
        created_by: options.userId,
        workspace_id: options.workspaceId,
        ...timestamps(),
      }),
      Comment: () => ({ author_id: options.userId, ...timestamps() }),
      Label: () => ({ team_id: null, color: "#b0b0bd", ...timestamps() }),
      Project: () => ({
        description: "",
        status: "planned",
        lead_id: null,
        initiative_id: null,
        target_date: null,
        sort_order: 0,
        ...timestamps(),
      }),
      Initiative: () => ({
        description: "",
        status: "planned",
        owner_id: null,
        target_date: null,
        ...timestamps(),
      }),
      IssueRelation: () => timestamps(),
      Service: () => ({
        icon: "server",
        color: "#b0b0bd",
        description: "",
        team_id: null,
        repository: null,
        paths: [],
        current_state: "",
        current_state_updated_at: null,
        ...timestamps(),
      }),
      Domain: () => ({ description: "", icon: "folder", color: "#b0b0bd", ...timestamps() }),
      Spec: () => ({
        body: "",
        status: "draft",
        service_ids: [],
        domain_id: null,
        project_id: null,
        created_by: options.userId,
        ...timestamps(),
      }),
      Decision: () => ({
        context: "",
        decision: "",
        consequences: "",
        status: "accepted",
        service_id: null,
        project_id: null,
        superseded_by_id: null,
        decided_on: null,
        created_by: options.userId,
        ...timestamps(),
      }),
      WorkflowState: () => ({ color: "#b0b0bd", position: 0, ...timestamps() }),
    },
    onReject: (_transaction, error) => options.onRefused(error.message),
  });
}
