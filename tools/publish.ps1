# publish.ps1 - initialize the local git repo and publish it to GitHub as a
# PUBLIC repository. Run from the project root.
# Pure ASCII: PowerShell 5.1 reads no-BOM scripts as ANSI.
#
# Usage:
#   powershell -NoProfile -File tools\publish.ps1
#       -> uses the GitHub CLI to create a public repo 'dsh-manager' and push
#          (install gh: winget install GitHub.cli, then 'gh auth login')
#
#   powershell -NoProfile -File tools\publish.ps1 -Remote <url>
#       -> classic route: push to an existing EMPTY repo you created on
#          github.com (do not add README / .gitignore / license there)
#
#   Optional: -RepoName <name> (default: dsh-manager)

param(
    [string]$Remote = "",
    [string]$RepoName = "dsh-manager"
)

$ErrorActionPreference = 'Stop'

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: git not found. Install Git for Windows first: https://git-scm.com/download/win"
    exit 1
}

# Git identity (one-time global config)
$userName = git config user.name
$userMail = git config user.email
if (-not $userName -or -not $userMail) {
    Write-Host "Git identity is not configured. Set it once, then re-run this script:"
    Write-Host '  git config --global user.name "Your Name"'
    Write-Host '  git config --global user.email "you@example.com"'
    exit 1
}

if (-not (Test-Path .git)) {
    git init | Out-Null
    Write-Host "Initialized git repository."
}

git add -A
$dirty = git status --porcelain
if ($dirty) {
    git commit -m "Initial commit: DSH desktop manager (Electron)" | Out-Null
    Write-Host "Created initial commit."
} else {
    Write-Host "Working tree is already committed."
}

git branch -M main

if ($Remote -eq "") {
    $gh = Get-Command gh -ErrorAction SilentlyContinue
    if ($gh) {
        $null = gh auth status 2>&1
        if ($LASTEXITCODE -eq 0) {
            Write-Host "Creating public repo '$RepoName' and pushing via GitHub CLI..."
            & gh repo create $RepoName --public --source=. --push
            Write-Host "Done. Repo: https://github.com/<your-account>/$RepoName"
            exit $LASTEXITCODE
        }
        Write-Host "gh is installed but not logged in. Run 'gh auth login', or use -Remote."
    }
    Write-Host ""
    Write-Host "No GitHub CLI available / not logged in. Two options:"
    Write-Host "  A) Recommended - install and login gh:"
    Write-Host "       winget install GitHub.cli"
    Write-Host "       gh auth login"
    Write-Host "       powershell -NoProfile -File tools\publish.ps1"
    Write-Host "  B) Manual:"
    Write-Host "     1. Create an EMPTY public repo named '$RepoName' on github.com"
    Write-Host "        (do NOT add README / .gitignore / license)."
    Write-Host "     2. Re-run with -Remote:"
    Write-Host "        powershell -NoProfile -File tools\publish.ps1 -Remote https://github.com/<you>/$RepoName.git"
    exit 0
}

git remote remove origin 2>$null
git remote add origin $Remote
git push -u origin main
Write-Host "Pushed to $Remote"
