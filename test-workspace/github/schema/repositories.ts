import { gql } from "graphql-tag";

export const repositories = gql`
  """
  A repository contains the content for a project.
  """
  type Repository implements Node & RepositoryNode & Starrable & Subscribable & UniformResourceLocatable {
    """
    A list of users that can be assigned to issues in this repository.
    """
    assignableUsers(
      first: Int
      after: String
      last: Int
      before: String
      query: String
    ): UserConnection!

    """
    A list of branch protection rules for this repository.
    """
    branchProtectionRules(
      first: Int
      after: String
      last: Int
      before: String
    ): BranchProtectionRuleConnection!

    """
    Returns the code of conduct for this repository.
    """
    codeOfConduct: CodeOfConduct

    """
    A list of collaborators associated with the repository.
    """
    collaborators(
      first: Int
      after: String
      last: Int
      before: String
      affiliation: CollaboratorAffiliation
      query: String
    ): RepositoryCollaboratorConnection

    """
    A list of commit comments associated with the repository.
    """
    commitComments(first: Int, after: String, last: Int, before: String): CommitCommentConnection!

    """
    Identifies the date and time when the object was created.
    """
    createdAt: DateTime!

    """
    Identifies the primary key from the database.
    """
    databaseId: Int

    """
    The Ref associated with the repository's default branch.
    """
    defaultBranchRef: Ref

    """
    Whether delete branch on merge is enabled.
    """
    deleteBranchOnMerge: Boolean!

    """
    A list of deploy keys for the repository.
    """
    deployKeys(first: Int, after: String, last: Int, before: String): DeployKeyConnection!

    """
    Deployments associated with the repository.
    """
    deployments(
      first: Int
      after: String
      last: Int
      before: String
      environments: [String!]
      orderBy: DeploymentOrder
    ): DeploymentConnection!

    """
    The description of the repository.
    """
    description: String

    """
    The description of the repository as HTML.
    """
    descriptionHTML: HTML!

    """
    Returns a single discussion from the current repository.
    """
    discussion(number: Int!): Discussion

    """
    A list of discussion categories for the repository.
    """
    discussionCategories(
      first: Int
      after: String
      last: Int
      before: String
      filterByAssignable: Boolean
    ): DiscussionCategoryConnection!

    """
    A list of discussions for the repository.
    """
    discussions(
      first: Int
      after: String
      last: Int
      before: String
      categoryId: ID
      orderBy: DiscussionOrder
      states: [DiscussionState!]
    ): DiscussionConnection!

    """
    The number of kilobytes this repository occupies on disk.
    """
    diskUsage: Int

    """
    Returns how many forks there are of this repository.
    """
    forkCount: Int!

    """
    Whether the repository is a fork.
    """
    isFork: Boolean!

    """
    Whether fork syncing is enabled.
    """
    forkingAllowed: Boolean!

    """
    A list of forks associated with the repository.
    """
    forks(
      first: Int
      after: String
      last: Int
      before: String
      orderBy: RepositoryOrder
      affiliations: [RepositoryAffiliation]
      isLocked: Boolean
      privacy: RepositoryPrivacy
    ): RepositoryConnection!

    """
    Whether the repository has issues enabled.
    """
    hasIssuesEnabled: Boolean!

    """
    Whether the repository has projects enabled.
    """
    hasProjectsEnabled: Boolean!

    """
    Whether the repository has wiki enabled.
    """
    hasWikiEnabled: Boolean!

    """
    The repository's URL.
    """
    homepageUrl: URI

    id: ID!

    """
    Whether the repository is archived.
    """
    isArchived: Boolean!

    """
    Whether the repository is blank.
    """
    isBlankIssuesEnabled: Boolean!

    """
    Whether the repository is disabled.
    """
    isDisabled: Boolean!

    """
    Whether the repository is empty.
    """
    isEmpty: Boolean!

    """
    Whether the repository is locked.
    """
    isLocked: Boolean!

    """
    Whether the repository is a mirror.
    """
    isMirror: Boolean!

    """
    Whether the repository is private.
    """
    isPrivate: Boolean!

    """
    Whether the repository is a template.
    """
    isTemplate: Boolean!

    """
    Returns a single issue from the current repository.
    """
    issue(number: Int!): Issue

    """
    Returns a single issue or pull request from the current repository.
    """
    issueOrPullRequest(number: Int!): IssueOrPullRequest

    """
    A list of issues associated with the repository.
    """
    issues(
      first: Int
      after: String
      last: Int
      before: String
      states: [IssueState!]
      labels: [String!]
      orderBy: IssueOrder
      filterBy: IssueFilters
    ): IssueConnection!

    """
    Returns a single label by name.
    """
    label(name: String!): Label

    """
    A list of labels for the repository.
    """
    labels(
      first: Int
      after: String
      last: Int
      before: String
      orderBy: LabelOrder
      query: String
    ): LabelConnection

    """
    A list of languages for the repository.
    """
    languages(
      first: Int
      after: String
      last: Int
      before: String
      orderBy: LanguageOrder
    ): LanguageConnection

    """
    The license for the repository.
    """
    licenseInfo: License

    """
    The reason the repository has been locked.
    """
    lockReason: RepositoryLockReason

    """
    A list of users who have starred this repository.
    """
    mentionableUsers(
      first: Int
      after: String
      last: Int
      before: String
      query: String
    ): UserConnection!

    """
    Whether to merge pull requests with a commit.
    """
    mergeCommitAllowed: Boolean!

    """
    Whether to merge pull requests with a merge commit.
    """
    mergeCommitMessage: MergeCommitMessage!

    """
    Whether to merge pull requests with a merge commit.
    """
    mergeCommitTitle: MergeCommitTitle!

    """
    Returns a single milestone from the current repository.
    """
    milestone(number: Int!): Milestone

    """
    A list of milestones for the repository.
    """
    milestones(
      first: Int
      after: String
      last: Int
      before: String
      states: [MilestoneState!]
      orderBy: MilestoneOrder
      query: String
    ): MilestoneConnection

    """
    The name of the repository.
    """
    name: String!

    """
    The repository's name with owner.
    """
    nameWithOwner: String!

    """
    A Git object in the repository.
    """
    object(oid: GitObjectID, expression: String): GitObject

    """
    The owner of the repository.
    """
    owner: RepositoryOwner!

    """
    The repository parent, if this is a fork.
    """
    parent: Repository

    """
    The primary language of the repository.
    """
    primaryLanguage: Language

    """
    Returns a single project from the current repository.
    """
    projectV2(number: Int!): ProjectV2

    """
    A list of projects for the repository.
    """
    projectsV2(
      first: Int
      after: String
      last: Int
      before: String
      orderBy: ProjectV2Order
      query: String
    ): ProjectV2Connection!

    """
    Returns a single pull request from the current repository.
    """
    pullRequest(number: Int!): PullRequest

    """
    A list of pull requests associated with the repository.
    """
    pullRequests(
      first: Int
      after: String
      last: Int
      before: String
      states: [PullRequestState!]
      labels: [String!]
      headRefName: String
      baseRefName: String
      orderBy: IssueOrder
    ): PullRequestConnection!

    """
    Identifies when the repository was last pushed to.
    """
    pushedAt: DateTime

    """
    Whether rebase merge is allowed.
    """
    rebaseMergeAllowed: Boolean!

    """
    The Ref associated with a name.
    """
    ref(qualifiedName: String!): Ref

    """
    A list of refs for the repository.
    """
    refs(
      first: Int
      after: String
      last: Int
      before: String
      refPrefix: String!
      direction: OrderDirection
      orderBy: RefOrder
      query: String
    ): RefConnection

    """
    Returns a single release from the current repository.
    """
    release(tagName: String!): Release

    """
    A list of releases for the repository.
    """
    releases(
      first: Int
      after: String
      last: Int
      before: String
      orderBy: ReleaseOrder
    ): ReleaseConnection!

    """
    The repository this repository was forked from.
    """
    repository: Repository!

    """
    The HTTP path for the repository.
    """
    resourcePath: URI!

    """
    A description of the repository.
    """
    shortDescriptionHTML(limit: Int = 200): HTML!

    """
    Whether squash merge is allowed.
    """
    squashMergeAllowed: Boolean!

    """
    The default commit message for squash merges.
    """
    squashMergeCommitMessage: SquashMergeCommitMessage!

    """
    The default commit title for squash merges.
    """
    squashMergeCommitTitle: SquashMergeCommitTitle!

    """
    The SSH URL to clone this repository.
    """
    sshUrl: GitSSHRemote!

    """
    Number of stars on this repository.
    """
    stargazerCount: Int!

    """
    A list of users who have starred the repository.
    """
    stargazers(
      first: Int
      after: String
      last: Int
      before: String
      orderBy: StarOrder
    ): StargazerConnection!

    """
    Returns a list of all submodules in this repository.
    """
    submodules(first: Int, after: String, last: Int, before: String): SubmoduleConnection!

    """
    Temporary clone token for the repository.
    """
    tempCloneToken: String

    """
    The repository from which this repository was templated.
    """
    templateRepository: Repository

    """
    A list of topics associated with the repository.
    """
    topics(first: Int, after: String, last: Int, before: String): RepositoryTopicConnection!

    """
    Identifies the date and time when the object was last updated.
    """
    updatedAt: DateTime!

    """
    The HTTP URL for the repository.
    """
    url: URI!

    """
    Whether the repository has branch protection.
    """
    usesCustomOpenGraphImage: Boolean!

    """
    Whether the viewer can administer the repository.
    """
    viewerCanAdminister: Boolean!

    """
    Whether the viewer can create projects in the repository.
    """
    viewerCanCreateProjects: Boolean!

    """
    Whether the viewer can subscribe to the repository.
    """
    viewerCanSubscribe: Boolean!

    """
    Whether the viewer can update topics.
    """
    viewerCanUpdateTopics: Boolean!

    """
    The viewer's default merge method.
    """
    viewerDefaultCommitEmail: String

    """
    The viewer's default merge method.
    """
    viewerDefaultMergeMethod: PullRequestMergeMethod!

    """
    Whether the viewer has starred the repository.
    """
    viewerHasStarred: Boolean!

    """
    The viewer's permission on the repository.
    """
    viewerPermission: RepositoryPermission

    """
    A list of emails the viewer can commit with.
    """
    viewerPossibleCommitEmails: [String!]

    """
    The viewer's subscription state.
    """
    viewerSubscription: SubscriptionState

    """
    The visibility of the repository.
    """
    visibility: RepositoryVisibility!

    """
    A list of users watching the repository.
    """
    watchers(first: Int, after: String, last: Int, before: String): UserConnection!

    """
    A list of workflows for the repository.
    """
    workflows(first: Int, after: String, last: Int, before: String): WorkflowConnection!
  }

  """
  A Git reference.
  """
  type Ref implements Node {
    """
    A list of pull requests with this ref as the head.
    """
    associatedPullRequests(
      first: Int
      after: String
      last: Int
      before: String
      states: [PullRequestState!]
      labels: [String!]
      headRefName: String
      baseRefName: String
      orderBy: IssueOrder
    ): PullRequestConnection!

    """
    Branch protection rules for this ref.
    """
    branchProtectionRule: BranchProtectionRule

    """
    Compares the current ref to another ref.
    """
    compare(headRef: String!): Comparison

    id: ID!

    """
    The ref name.
    """
    name: String!

    """
    The ref prefix.
    """
    prefix: String!

    """
    The ref name without the prefix.
    """
    refUpdateRule: RefUpdateRule

    """
    The repository the ref belongs to.
    """
    repository: Repository!

    """
    The object the ref points to.
    """
    target: GitObject
  }

  """
  A comparison between two refs.
  """
  type Comparison {
    """
    The number of commits ahead.
    """
    aheadBy: Int!

    """
    The number of commits behind.
    """
    behindBy: Int!

    """
    The base ref.
    """
    baseTarget: GitObject!

    """
    The head ref.
    """
    headTarget: GitObject!

    """
    The commits between the base and head.
    """
    commits(first: Int, after: String, last: Int, before: String): CommitConnection!

    """
    The comparison status.
    """
    status: ComparisonStatus!
  }

  """
  The status of a comparison.
  """
  enum ComparisonStatus {
    """
    The head is ahead of the base.
    """
    AHEAD

    """
    The head is behind the base.
    """
    BEHIND

    """
    The head and base have diverged.
    """
    DIVERGED

    """
    The head and base are identical.
    """
    IDENTICAL
  }

  """
  A branch protection rule.
  """
  type BranchProtectionRule implements Node {
    """
    A list of branch protection rule conflicts.
    """
    branchProtectionRuleConflicts(
      first: Int
      after: String
      last: Int
      before: String
    ): BranchProtectionRuleConflictConnection!

    """
    The actor who created this rule.
    """
    creator: Actor

    """
    Identifies the primary key from the database.
    """
    databaseId: Int

    """
    Will new commits pushed to matching branches dismiss pull request reviews.
    """
    dismissesStaleReviews: Boolean!

    id: ID!

    """
    Can admins override rules.
    """
    isAdminEnforced: Boolean!

    """
    The ref name pattern.
    """
    pattern: String!

    """
    Whether commits on matching branches require a signature.
    """
    requiresCommitSignatures: Boolean!

    """
    Are reviews required on matching branches.
    """
    requiresApprovingReviews: Boolean!

    """
    The number of reviews required.
    """
    requiredApprovingReviewCount: Int

    """
    List of required status check contexts.
    """
    requiredStatusCheckContexts: [String]

    """
    Whether status checks are required.
    """
    requiresStatusChecks: Boolean!

    """
    Whether branches must be up to date before merging.
    """
    requiresStrictStatusChecks: Boolean!

    """
    Whether conversations must be resolved.
    """
    requiresConversationResolution: Boolean!

    """
    Whether linear history is required.
    """
    requiresLinearHistory: Boolean!

    """
    Whether pushing is restricted.
    """
    restrictsPushes: Boolean!

    """
    Whether review dismissals are restricted.
    """
    restrictsReviewDismissals: Boolean!
  }

  """
  The connection type for BranchProtectionRule.
  """
  type BranchProtectionRuleConnection {
    edges: [BranchProtectionRuleEdge]
    nodes: [BranchProtectionRule]
    pageInfo: PageInfo!
    totalCount: Int!
  }

  type BranchProtectionRuleEdge {
    cursor: String!
    node: BranchProtectionRule
  }

  type BranchProtectionRuleConflictConnection {
    edges: [BranchProtectionRuleConflictEdge]
    nodes: [BranchProtectionRuleConflict]
    pageInfo: PageInfo!
    totalCount: Int!
  }

  type BranchProtectionRuleConflictEdge {
    cursor: String!
    node: BranchProtectionRuleConflict
  }

  type BranchProtectionRuleConflict {
    branchProtectionRule: BranchProtectionRule
    conflictingBranchProtectionRule: BranchProtectionRule
    ref: Ref
  }

  """
  A label for categorizing objects.
  """
  type Label implements Node {
    """
    Identifies the label color.
    """
    color: String!

    """
    Identifies the date and time when the label was created.
    """
    createdAt: DateTime

    """
    A brief description of this label.
    """
    description: String

    id: ID!

    """
    Whether this label is a default.
    """
    isDefault: Boolean!

    """
    A list of issues with this label.
    """
    issues(
      first: Int
      after: String
      last: Int
      before: String
      states: [IssueState!]
      labels: [String!]
      orderBy: IssueOrder
      filterBy: IssueFilters
    ): IssueConnection!

    """
    The label name.
    """
    name: String!

    """
    A list of pull requests with this label.
    """
    pullRequests(
      first: Int
      after: String
      last: Int
      before: String
      states: [PullRequestState!]
      labels: [String!]
      headRefName: String
      baseRefName: String
      orderBy: IssueOrder
    ): PullRequestConnection!

    """
    The repository this label belongs to.
    """
    repository: Repository!

    """
    The HTTP path for the label.
    """
    resourcePath: URI!

    """
    Identifies the date and time when the label was last updated.
    """
    updatedAt: DateTime

    """
    The HTTP URL for the label.
    """
    url: URI!
  }

  """
  A milestone within a repository.
  """
  type Milestone implements Node & Closable & UniformResourceLocatable {
    """
    Whether the milestone is closed.
    """
    closed: Boolean!

    """
    Identifies the date and time when the object was closed.
    """
    closedAt: DateTime

    """
    Identifies the date and time when the object was created.
    """
    createdAt: DateTime!

    """
    The actor who created this milestone.
    """
    creator: Actor

    """
    A description for the milestone.
    """
    description: String

    """
    The due date for the milestone.
    """
    dueOn: DateTime

    id: ID!

    """
    A list of issues with this milestone.
    """
    issues(
      first: Int
      after: String
      last: Int
      before: String
      states: [IssueState!]
      labels: [String!]
      orderBy: IssueOrder
      filterBy: IssueFilters
    ): IssueConnection!

    """
    The milestone number.
    """
    number: Int!

    """
    The percentage of issues and pull requests closed.
    """
    progressPercentage: Float!

    """
    A list of pull requests with this milestone.
    """
    pullRequests(
      first: Int
      after: String
      last: Int
      before: String
      states: [PullRequestState!]
      labels: [String!]
      headRefName: String
      baseRefName: String
      orderBy: IssueOrder
    ): PullRequestConnection!

    """
    The repository this milestone belongs to.
    """
    repository: Repository!

    """
    The HTTP path for the milestone.
    """
    resourcePath: URI!

    """
    The state of the milestone.
    """
    state: MilestoneState!

    """
    The milestone title.
    """
    title: String!

    """
    Identifies the date and time when the object was last updated.
    """
    updatedAt: DateTime!

    """
    The HTTP URL for the milestone.
    """
    url: URI!
  }

  """
  The connection type for RepositoryCollaborator.
  """
  type RepositoryCollaboratorConnection {
    edges: [RepositoryCollaboratorEdge]
    nodes: [User]
    pageInfo: PageInfo!
    totalCount: Int!
  }

  type RepositoryCollaboratorEdge {
    cursor: String!
    node: User!
    permission: RepositoryPermission!
    permissionSources: [PermissionSource!]
  }

  type PermissionSource {
    organization: Organization!
    permission: DefaultRepositoryPermissionField!
    source: PermissionGranter!
  }

  union PermissionGranter = Organization | Repository | Team

  """
  Collaborator affiliation.
  """
  enum CollaboratorAffiliation {
    OUTSIDE
    DIRECT
    ALL
  }

  """
  A code of conduct for a repository.
  """
  type CodeOfConduct implements Node {
    body: String
    id: ID!
    key: String!
    name: String!
    resourcePath: URI
    url: URI
  }

  """
  A deploy key.
  """
  type DeployKey implements Node {
    createdAt: DateTime!
    id: ID!
    key: String!
    readOnly: Boolean!
    title: String!
    verified: Boolean!
  }

  type DeployKeyConnection {
    edges: [DeployKeyEdge]
    nodes: [DeployKey]
    pageInfo: PageInfo!
    totalCount: Int!
  }

  type DeployKeyEdge {
    cursor: String!
    node: DeployKey
  }

  """
  A language.
  """
  type Language implements Node {
    color: String
    id: ID!
    name: String!
  }

  type LanguageConnection {
    edges: [LanguageEdge]
    nodes: [Language]
    pageInfo: PageInfo!
    totalCount: Int!
    totalSize: Int!
  }

  type LanguageEdge {
    cursor: String!
    node: Language!
    size: Int!
  }

  """
  A repository license.
  """
  type License implements Node {
    body: String!
    conditions: [LicenseRule]!
    description: String
    featured: Boolean!
    hidden: Boolean!
    id: ID!
    implementation: String
    key: String!
    limitations: [LicenseRule]!
    name: String!
    nickname: String
    permissions: [LicenseRule]!
    pseudoLicense: Boolean!
    spdxId: String
    url: URI
  }

  type LicenseRule {
    description: String!
    key: String!
    label: String!
  }

  """
  The reason a repository is locked.
  """
  enum RepositoryLockReason {
    BILLING
    MIGRATING
    MOVING
    RENAME
    TRADE_RESTRICTION
  }

  """
  The merge commit message.
  """
  enum MergeCommitMessage {
    PR_TITLE
    PR_BODY
    BLANK
  }

  """
  The merge commit title.
  """
  enum MergeCommitTitle {
    PR_TITLE
    MERGE_MESSAGE
  }

  """
  The squash merge commit message.
  """
  enum SquashMergeCommitMessage {
    PR_BODY
    COMMIT_MESSAGES
    BLANK
  }

  """
  The squash merge commit title.
  """
  enum SquashMergeCommitTitle {
    PR_TITLE
    COMMIT_OR_PR_TITLE
  }

  """
  A Git SSH remote.
  """
  scalar GitSSHRemote

  """
  A repository submodule.
  """
  type Submodule {
    branch: String
    gitUrl: URI!
    name: String!
    path: String!
    subprojectCommitOid: GitObjectID
  }

  type SubmoduleConnection {
    edges: [SubmoduleEdge]
    nodes: [Submodule]
    pageInfo: PageInfo!
    totalCount: Int!
  }

  type SubmoduleEdge {
    cursor: String!
    node: Submodule
  }

  """
  A repository topic.
  """
  type Topic implements Node & Starrable {
    id: ID!
    name: String!
    relatedTopics(first: Int): [Topic!]!
    stargazerCount: Int!
    stargazers(
      first: Int
      after: String
      last: Int
      before: String
      orderBy: StarOrder
    ): StargazerConnection!
    viewerHasStarred: Boolean!
  }

  type RepositoryTopic implements Node & UniformResourceLocatable {
    id: ID!
    resourcePath: URI!
    topic: Topic!
    url: URI!
  }

  type RepositoryTopicConnection {
    edges: [RepositoryTopicEdge]
    nodes: [RepositoryTopic]
    pageInfo: PageInfo!
    totalCount: Int!
  }

  type RepositoryTopicEdge {
    cursor: String!
    node: RepositoryTopic
  }

  input IssueFilters {
    assignee: String
    createdBy: String
    labels: [String!]
    mentioned: String
    milestone: String
    milestoneNumber: String
    since: DateTime
    states: [IssueState!]
    viewerSubscribed: Boolean
  }

  input DiscussionOrder {
    field: DiscussionOrderField!
    direction: OrderDirection!
  }

  enum DiscussionOrderField {
    CREATED_AT
    UPDATED_AT
  }

  type DiscussionCategory implements Node {
    createdAt: DateTime!
    description: String
    emoji: String!
    emojiHTML: HTML!
    id: ID!
    isAnswerable: Boolean!
    name: String!
    repository: Repository!
    slug: String!
    updatedAt: DateTime!
  }

  type DiscussionCategoryConnection {
    edges: [DiscussionCategoryEdge]
    nodes: [DiscussionCategory]
    pageInfo: PageInfo!
    totalCount: Int!
  }

  type DiscussionCategoryEdge {
    cursor: String!
    node: DiscussionCategory
  }

  input DeploymentOrder {
    field: DeploymentOrderField!
    direction: OrderDirection!
  }

  enum DeploymentOrderField {
    CREATED_AT
  }

  input RefOrder {
    field: RefOrderField!
    direction: OrderDirection!
  }

  enum RefOrderField {
    TAG_COMMIT_DATE
    ALPHABETICAL
  }

  input ProjectV2Order {
    field: ProjectV2OrderField!
    direction: OrderDirection!
  }

  enum ProjectV2OrderField {
    TITLE
    NUMBER
    UPDATED_AT
    CREATED_AT
  }

  type CommitComment implements Node & Comment & Reactable & RepositoryNode & UniformResourceLocatable {
    author: Actor
    authorAssociation: CommentAuthorAssociation!
    body: String!
    bodyHTML: HTML!
    bodyText: String!
    commit: Commit
    createdAt: DateTime!
    createdViaEmail: Boolean!
    databaseId: Int
    editor: Actor
    id: ID!
    includesCreatedEdit: Boolean!
    isMinimized: Boolean!
    lastEditedAt: DateTime
    minimizedReason: String
    path: String
    position: Int
    publishedAt: DateTime
    reactions(
      first: Int
      after: String
      last: Int
      before: String
      content: ReactionContent
      orderBy: ReactionOrder
    ): ReactionConnection!
    repository: Repository!
    resourcePath: URI!
    updatedAt: DateTime!
    url: URI!
    viewerCanDelete: Boolean!
    viewerCanMinimize: Boolean!
    viewerCanReact: Boolean!
    viewerCanUpdate: Boolean!
    viewerDidAuthor: Boolean!
  }

  type CommitCommentConnection {
    edges: [CommitCommentEdge]
    nodes: [CommitComment]
    pageInfo: PageInfo!
    totalCount: Int!
  }

  type CommitCommentEdge {
    cursor: String!
    node: CommitComment
  }

  enum CommentAuthorAssociation {
    MEMBER
    OWNER
    MANNEQUIN
    COLLABORATOR
    CONTRIBUTOR
    FIRST_TIME_CONTRIBUTOR
    FIRST_TIMER
    NONE
  }
`;
