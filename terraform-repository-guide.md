# Adding New Repositories with Terraform and Atlantis

This guide walks through the process of adding a new GitHub repository using Terraform and managing it with Atlantis automation.

## Prerequisites

- Access to the [skip-terraform repository](https://github.com/skip-mev/skip-terraform)
- GitHub permissions to create repositories in the skip-mev organization
- Atlantis server configured and running
- Terraform CLI installed locally

## Step 1: Clone the Terraform Repository

Start by cloning the terraform configuration repository:

```bash
git clone https://github.com/skip-mev/skip-terraform.git
cd skip-terraform
```

## Step 2: Create Repository Configuration

### Basic Repository Structure

Add a new GitHub repository resource to your Terraform configuration. Create or edit the appropriate `.tf` file:

```hcl
resource "github_repository" "your_repo_name" {
  name        = "your-repo-name"
  description = "Brief description of your repository"
  
  # Repository visibility
  private = true  # or false for public repos
  
  # Repository features
  has_issues   = true
  has_wiki     = false
  has_projects = true
  
  # Initialize repository
  auto_init          = true
  gitignore_template = "Node"  # Choose appropriate template
  license_template   = "mit"   # or your preferred license
  
  # Merge settings
  allow_merge_commit     = true
  allow_squash_merge     = true
  allow_rebase_merge     = true
  allow_auto_merge       = false
  delete_branch_on_merge = true
  
  # Security settings
  vulnerability_alerts         = true
  disable_vulnerability_alerts = false
  
  # Squash merge configuration (like the wfchain example)
  squash_merge_commit_message = "COMMIT_MESSAGES"
  squash_merge_commit_title   = "COMMIT_OR_PR_TITLE"
}
```

### Advanced Configuration Options

For repositories that need specific settings, you can add:

```hcl
resource "github_repository" "advanced_repo" {
  name = "advanced-repo-name"
  
  # ... basic settings ...
  
  # Topics for discoverability
  topics = ["blockchain", "cosmos", "ibc", "ethereum"]
  
  # Branch protection (optional)
  default_branch = "main"
  
  # Archive settings
  archived           = false
  archive_on_destroy = false
  
  # Pages settings (if needed)
  pages {
    source {
      branch = "main"
      path   = "/docs"
    }
  }
  
  # Security and analysis
  security_and_analysis {
    secret_scanning {
      status = "enabled"
    }
    secret_scanning_push_protection {
      status = "enabled"
    }
  }
}
```

## Step 3: Configure Branch Protection

Add branch protection rules for important branches:

```hcl
resource "github_branch_protection" "main_protection" {
  repository_id = github_repository.your_repo_name.node_id
  pattern       = "main"
  
  required_status_checks {
    strict = true
    contexts = [
      "ci/tests",
      "ci/lint",
      "ci/build"
    ]
  }
  
  required_pull_request_reviews {
    dismiss_stale_reviews           = true
    required_approving_review_count = 1
    require_code_owner_reviews      = true
  }
  
  enforce_admins = false
  allows_deletions = false
  allows_force_pushes = false
}
```

## Step 4: Set Up Team Access (Optional)

If you need to grant specific team access:

```hcl
resource "github_team_repository" "repo_access" {
  team_id    = data.github_team.dev_team.id
  repository = github_repository.your_repo_name.name
  permission = "push"  # or "pull", "triage", "maintain", "admin"
}

# Reference existing team
data "github_team" "dev_team" {
  slug = "developers"
}
```

## Step 5: Configure Atlantis Integration

### Atlantis Server Configuration

Ensure your Atlantis server is configured to allow the new repository. Update your Atlantis server configuration:

```yaml
# atlantis.yaml (server config)
repos:
  - id: github.com/skip-mev/your-repo-name
    allowed_overrides: [workflow]
    allow_custom_workflows: true
```

### Repository Atlantis Configuration

Create an `atlantis.yaml` file in your new repository root:

```yaml
# atlantis.yaml (repository config)
version: 3
projects:
  - name: infrastructure
    dir: .
    workspace: default
    terraform_version: v1.5.0
    autoplan:
      when_modified: ["*.tf", "*.tfvars"]
      enabled: true
    apply_requirements: ["approved", "mergeable"]
    workflow: default
```

### Webhook Configuration

The webhook should be automatically configured if using GitHub App authentication, but you can manually set it up:

1. Go to your repository **Settings** → **Webhooks**
2. Add webhook with:
   - **Payload URL**: `https://your-atlantis-server.com/events`
   - **Content type**: `application/json`
   - **Secret**: Your webhook secret
   - **Events**: Pull requests, Pushes, Pull request reviews, Issue comments

## Step 6: Apply Terraform Configuration

### Plan and Apply

1. Initialize Terraform (if not already done):
   ```bash
   terraform init
   ```

2. Plan the changes to review what will be created:
   ```bash
   terraform plan
   ```

3. Apply the configuration:
   ```bash
   terraform apply
   ```

### Using Atlantis Workflow

If using Atlantis for the terraform repository itself:

1. Create a new branch:
   ```bash
   git checkout -b add-new-repo
   ```

2. Commit your changes:
   ```bash
   git add .
   git commit -m "feat: add new repository configuration"
   ```

3. Push and create a pull request:
   ```bash
   git push origin add-new-repo
   ```

4. Atlantis will automatically run `terraform plan`
5. Review the plan output in the PR comments
6. Comment `atlantis apply` to execute the changes
7. Merge the PR after successful apply

## Step 7: Verify Repository Creation

After applying:

1. **Check GitHub**: Verify the repository exists with correct settings
2. **Test Atlantis**: Create a test PR in the new repository to ensure Atlantis responds
3. **Verify Permissions**: Ensure team access and branch protections are working

## Common Configuration Examples

### Private Repository with Standard Settings
```hcl
resource "github_repository" "standard_private" {
  name        = "private-service"
  description = "Internal service repository"
  private     = true
  
  has_issues = true
  has_wiki   = false
  auto_init  = true
  
  gitignore_template = "Go"
  license_template   = "mit"
  
  allow_squash_merge     = true
  allow_merge_commit     = false
  allow_rebase_merge     = false
  delete_branch_on_merge = true
  
  squash_merge_commit_message = "COMMIT_MESSAGES"
  squash_merge_commit_title   = "COMMIT_OR_PR_TITLE"
}
```

### Public Open Source Repository
```hcl
resource "github_repository" "open_source" {
  name        = "awesome-tool"
  description = "An awesome open source tool"
  private     = false
  
  has_issues   = true
  has_wiki     = true
  has_projects = true
  
  topics = ["golang", "cli", "automation"]
  
  auto_init        = true
  license_template = "apache-2.0"
  
  vulnerability_alerts = true
}
```

## Troubleshooting

### Common Issues

1. **Repository already exists**: Check if the repository name is already taken
2. **Permission denied**: Ensure your GitHub token has repository creation permissions
3. **Atlantis not responding**: Check webhook configuration and server logs
4. **Branch protection conflicts**: Ensure required status checks match your CI/CD setup

### Useful Commands

```bash
# Check Terraform state
terraform state list

# Import existing repository (if needed)
terraform import github_repository.existing_repo repository-name

# Refresh state
terraform refresh

# Destroy repository (use with caution!)
terraform destroy -target=github_repository.repo_name
```

## Best Practices

1. **Naming Convention**: Use consistent naming patterns (kebab-case recommended)
2. **Security First**: Enable vulnerability alerts and secret scanning
3. **Branch Protection**: Always protect main/master branches
4. **Documentation**: Include clear descriptions and topics
5. **Team Access**: Use teams for access management instead of individual users
6. **Atlantis Planning**: Always review terraform plans before applying
7. **State Management**: Keep terraform state secure and backed up

## Next Steps

After creating your repository:

1. Set up CI/CD workflows (GitHub Actions)
2. Configure repository-specific Atlantis workflows if needed
3. Add repository to monitoring and alerting systems
4. Update team documentation with new repository information
5. Set up any required integrations (Slack notifications, etc.)

---

For questions or issues, refer to:
- [Terraform GitHub Provider Documentation](https://registry.terraform.io/providers/integrations/github/latest/docs)
- [Atlantis Documentation](https://www.runatlantis.io/docs/)
- Internal team documentation and runbooks