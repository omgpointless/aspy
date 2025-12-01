# GITHUB_TOKEN cannot create releases despite `contents: write` permission

## Summary

`github-actions[bot]` receives HTTP 422 "author_id does not have push access" when attempting to create a release via the GitHub API, despite having `contents: write` permission and being able to perform other write operations (pushing tags).

## Environment

- Repository: Personal account (not organization)
- Repository visibility: **Private**
- Repository age: Newly created
- Workflow permissions setting: "Read and write permissions" (verified in Settings → Actions → General)
- "Allow GitHub Actions to create and approve pull requests": Enabled

## Reproduction Steps

1. Create a new repository
2. Add a release workflow with `permissions: contents: write`
3. Push a version tag to trigger the workflow
4. Workflow fails when attempting to create release

## Workflow Configuration

```yaml
permissions:
  contents: write

jobs:
  release:
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4

      - name: Create GitHub Release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          gh release create "v0.1.0" --title "v0.1.0" --notes "Release notes"
```

## Error Message

```
HTTP 422: Validation Failed (https://api.github.com/repos/OWNER/REPO/releases)
author_id does not have push access to OWNER/REPO
```

## Debugging Performed

### 1. Verified token identity
```yaml
- name: Debug token identity
  run: |
    echo "Actor: ${{ github.actor }}"
    echo "Triggering actor: ${{ github.triggering_actor }}"
    gh auth status
  env:
    GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

**Output:**
```
Actor: omgpointless
Triggering actor: omgpointless
github.com
  ✓ Logged in to github.com account github-actions[bot] (GH_TOKEN)
  - Active account: true
  - Git operations protocol: https
  - Token: ghs_****
```

### 2. Tested other write operations

**Pushing tags - WORKS:**
```yaml
- name: Try to create a test tag
  run: |
    git config user.name "github-actions[bot]"
    git config user.email "github-actions[bot]@users.noreply.github.com"
    git tag test-bot-access
    git push origin test-bot-access
  env:
    GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```
Result: Success ✓

**Creating release for bot-created tag - FAILS:**
```yaml
- name: Try to create a release for bot-created tag
  run: |
    gh release create test-bot-access-2 --title "Test" --notes "Test"
  env:
    GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```
Result: Same 422 error ✗

### 3. Tested local authentication

```bash
gh release create v0.1.0-alpha --title "v0.1.0-alpha" --notes "Test release" --prerelease
```
Result: Success ✓ (release created successfully with user's personal authentication)

### 4. Tested multiple release methods

- `softprops/action-gh-release@v2` - Same error
- `gh release create` (GitHub CLI) - Same error

Both fail with identical "author_id does not have push access" error.

## Findings

| Operation | GITHUB_TOKEN | Personal Auth |
|-----------|--------------|---------------|
| Push tags | ✓ Works | ✓ Works |
| Push commits | ✓ Works | ✓ Works |
| Create releases | ✗ Fails (422) | ✓ Works |

## Workaround

Using a Personal Access Token (classic) with `repo` scope instead of `GITHUB_TOKEN` resolves the issue.

## Expected Behavior

`github-actions[bot]` should be able to create releases when:
1. Workflow declares `permissions: contents: write`
2. Repository settings allow "Read and write permissions" for workflows

## Actual Behavior

The Release API specifically rejects `github-actions[bot]` with "author_id does not have push access" while other write operations succeed with the same token.

## Additional Context

- No branch protection rules configured
- No tag protection rules configured
- No repository rulesets configured
- Issue persists across multiple workflow runs
- Issue occurs for both user-created tags and bot-created tags
- **Repository is private** - this may be relevant to the permission check behavior
- PAT with `repo` scope works, suggesting the Release API has different authorization requirements for `github-actions[bot]` vs user tokens on private repositories
