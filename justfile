binary := "target/release/cluster"

# Build optimized binary (LTO + size opt + strip — see [profile.release] in Cargo.toml)
build:
    cargo build --release
    @ls -lh {{binary}}

# Run with cargo (debug mode)
run *ARGS:
    cargo run -- {{ARGS}}

# Get current version from Cargo.toml
_get-version:
    @grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/'

# Get current git tag
_get-tag:
    @git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0"

# Release a new version
# Usage:
#   just release                    # Interactive menu
#   just release patch              # Bump patch (0.1.0 -> 0.1.1)
#   just release minor              # Bump minor (0.1.0 -> 0.2.0)
#   just release major              # Bump major (0.1.0 -> 1.0.0)
#   just release 1.2.3             # Set specific version
#   just release v1.2.3             # Set specific version (with v prefix)
#   just release patch --yes        # Auto-confirm
#   just release 1.2.3 -y           # Auto-confirm
release ARGS="":
    #!/usr/bin/env bash
    set -e
    
    # Parse arguments
    INPUT="{{ARGS}}"
    BUMP_TYPE=""
    NEW_VERSION=""
    AUTO_CONFIRM=""
    
    # Parse all arguments
    for arg in $INPUT; do
        if [[ "$arg" == "--yes" || "$arg" == "-y" ]]; then
            AUTO_CONFIRM="yes"
        elif [[ "$arg" == "patch" || "$arg" == "minor" || "$arg" == "major" ]]; then
            BUMP_TYPE="$arg"
        elif [[ "$arg" =~ ^v?[0-9]+\.[0-9]+\.[0-9]+.*$ ]]; then
            # Remove 'v' prefix if present
            NEW_VERSION="${arg#v}"
        elif [[ -n "$arg" ]]; then
            echo "Error: Unknown argument '$arg'"
            echo ""
            echo "Usage:"
            echo "  just release                    # Interactive menu"
            echo "  just release patch              # Bump patch version"
            echo "  just release minor              # Bump minor version"
            echo "  just release major              # Bump major version"
            echo "  just release 1.2.3              # Set specific version"
            echo "  just release v1.2.3             # Set specific version"
            echo "  just release patch --yes        # Auto-confirm"
            exit 1
        fi
    done
    
    # Get current version from Cargo.toml
    CURRENT_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    echo "Current Cargo.toml version: $CURRENT_VERSION"
    
    # Get current git tag
    CURRENT_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "v0.0.0")
    echo "Current git tag: $CURRENT_TAG"
    
    # If no arguments provided, show interactive menu
    if [[ -z "$BUMP_TYPE" && -z "$NEW_VERSION" ]]; then
        echo ""
        echo "Select release type:"
        echo "  1) patch  - Bump patch version ($CURRENT_VERSION -> $(echo $CURRENT_VERSION | awk -F. '{print $1"."$2"."($3+1)}'))"
        echo "  2) minor  - Bump minor version ($CURRENT_VERSION -> $(echo $CURRENT_VERSION | awk -F. '{print $1"."($2+1)".0"}'))"
        echo "  3) major  - Bump major version ($CURRENT_VERSION -> $(echo $CURRENT_VERSION | awk -F. '{print ($1+1)".0.0"}'))"
        echo "  4) custom - Enter specific version"
        echo ""
        echo -n "Enter choice [1-4]: "
        read -r choice
        
        case "$choice" in
            1|patch)
                BUMP_TYPE="patch"
                ;;
            2|minor)
                BUMP_TYPE="minor"
                ;;
            3|major)
                BUMP_TYPE="major"
                ;;
            4|custom)
                echo -n "Enter version (e.g., 1.2.3): "
                read -r NEW_VERSION
                if [[ ! "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+.*$ ]]; then
                    echo "Error: Invalid version format. Expected: x.y.z"
                    exit 1
                fi
                ;;
            *)
                echo "Aborted."
                exit 0
                ;;
        esac
    fi
    
    # Calculate new version if using bump type
    if [[ -n "$BUMP_TYPE" && -z "$NEW_VERSION" ]]; then
        VERSION=$CURRENT_VERSION
        MAJOR=$(echo "$VERSION" | cut -d. -f1)
        MINOR=$(echo "$VERSION" | cut -d. -f2)
        PATCH=$(echo "$VERSION" | cut -d. -f3)
        
        case "$BUMP_TYPE" in
            major)
                NEW_MAJOR=$((MAJOR + 1))
                NEW_VERSION="$NEW_MAJOR.0.0"
                ;;
            minor)
                NEW_VERSION="$MAJOR.$((MINOR + 1)).0"
                ;;
            patch)
                NEW_VERSION="$MAJOR.$MINOR.$((PATCH + 1))"
                ;;
        esac
    fi
    
    NEW_TAG="v$NEW_VERSION"
    
    echo ""
    echo "========================================"
    echo "Release Plan:"
    if [[ -n "$BUMP_TYPE" ]]; then
        echo "  Bump type: $BUMP_TYPE"
    else
        echo "  Bump type: custom"
    fi
    echo "  Old version: $CURRENT_VERSION"
    echo "  New version: $NEW_VERSION"
    echo "  New tag: $NEW_TAG"
    echo "========================================"
    echo ""
    
    # Check if tag already exists
    if git rev-parse "$NEW_TAG" >/dev/null 2>&1; then
        echo "Error: Tag $NEW_TAG already exists!"
        exit 1
    fi
    
    # Check for uncommitted changes
    if ! git diff-index --quiet HEAD --; then
        echo "Error: You have uncommitted changes. Please commit or stash them first."
        exit 1
    fi
    
    # Confirmation
    if [[ "$AUTO_CONFIRM" != "yes" ]]; then
        echo -n "Do you want to proceed? [y/N] "
        read -r response
        if [[ ! "$response" =~ ^[Yy]$ ]]; then
            echo "Aborted."
            exit 0
        fi
    else
        echo "Auto-confirmed with --yes flag"
    fi
    
    # Update Cargo.toml
    echo "Updating Cargo.toml..."
    sed -i.bak "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml
    rm -f Cargo.toml.bak
    
    # Update Cargo.lock
    echo "Updating Cargo.lock..."
    cargo update -w >/dev/null 2>&1 || true
    
    # Commit version bump
    echo "Committing version bump..."
    git add Cargo.toml Cargo.lock
    git commit -m "chore: bump version to $NEW_VERSION"
    
    # Create tag
    echo "Creating tag $NEW_TAG..."
    git tag -a "$NEW_TAG" -m "Release $NEW_TAG"
    
    # Push to remote
    echo "Pushing to remote..."
    git push origin main
    git push origin "$NEW_TAG"
    
    echo ""
    echo "✅ Release $NEW_TAG created and pushed!"
    echo "GitHub Actions will now build and publish the release."
    echo ""
    echo "You can monitor the build at:"
    echo "  https://github.com/$(git remote get-url origin | sed 's/.*github.com[:/]\([^/]*\/[^/]*\)\.git$/\1/')/actions"

# Publish current version without incrementing
# Usage:
#   just publish              # Publish current version (asks for confirmation)
#   just publish --yes        # Publish current version (auto-confirm)
publish CONFIRM="":
    #!/usr/bin/env bash
    set -e
    
    AUTO_CONFIRM="{{CONFIRM}}"
    
    # Get current version from Cargo.toml
    CURRENT_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    echo "Current Cargo.toml version: $CURRENT_VERSION"
    
    # Get current git tag
    CURRENT_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
    
    if [[ -n "$CURRENT_TAG" ]]; then
        echo "Current git tag: $CURRENT_TAG"
    else
        echo "No existing git tags found"
    fi
    
    NEW_TAG="v$CURRENT_VERSION"
    
    # Check if tag already exists
    if git rev-parse "$NEW_TAG" >/dev/null 2>&1; then
        echo ""
        echo "❌ Error: Tag $NEW_TAG already exists!"
        echo ""
        echo "Options:"
        echo "  1. Use 'just release' to bump to a new version"
        echo "  2. Delete the existing tag first: git tag -d $NEW_TAG && git push origin :refs/tags/$NEW_TAG"
        exit 1
    fi
    
    # Check for uncommitted changes
    if ! git diff-index --quiet HEAD --; then
        echo "Error: You have uncommitted changes. Please commit or stash them first."
        exit 1
    fi
    
    echo ""
    echo "========================================"
    echo "Publish Plan:"
    echo "  Version: $CURRENT_VERSION"
    echo "  Tag: $NEW_TAG"
    echo "========================================"
    echo ""
    
    # Confirmation
    if [[ "$AUTO_CONFIRM" != "--yes" && "$AUTO_CONFIRM" != "-y" ]]; then
        echo -n "Publish version $CURRENT_VERSION? [y/N] "
        read -r response
        if [[ ! "$response" =~ ^[Yy]$ ]]; then
            echo "Aborted."
            exit 0
        fi
    else
        echo "Auto-confirmed with --yes flag"
    fi
    
    # Create tag
    echo "Creating tag $NEW_TAG..."
    git tag -a "$NEW_TAG" -m "Release $NEW_TAG"
    
    # Push to remote
    echo "Pushing to remote..."
    git push origin "$NEW_TAG"
    
    echo ""
    echo "✅ Tag $NEW_TAG pushed!"
    echo "GitHub Actions will now build and publish the release."
    echo ""
    echo "You can monitor the build at:"
    echo "  https://github.com/$(git remote get-url origin | sed 's/.*github.com[:/]\([^/]*\/[^/]*\)\.git$/\1/')/actions"

# Show current version info
version:
    @echo "Cargo.toml version: $(grep '^version' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')"
    @echo "Git tag: $(git describe --tags --abbrev=0 2>/dev/null || echo 'No tags found')"

# Dry run release (shows what would happen without making changes)
dry-run:
    @echo "=== DRY RUN ==="
    @echo "This would show the interactive menu"
    @echo "=== END DRY RUN ==="
    @echo "Note: This was a dry run. No changes were made."
