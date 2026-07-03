# Canonical Homebrew Cask for Agent Voice App.
#
# This file is the source of truth. The `.github/workflows/update-homebrew-cask.yml`
# workflow copies it into the tap repo (abhi-singhs/homebrew-tap) as
# `Casks/agent-voice-app.rb` on every published GitHub Release, filling in the
# real `version` and `sha256` for the macOS (Apple Silicon) DMG.
#
# The __VERSION__ / __SHA256__ tokens below are placeholders replaced by CI.
# Install once the tap exists:
#   brew tap abhi-singhs/tap
#   brew install --cask agent-voice-app
cask "agent-voice-app" do
  version "__VERSION__"
  sha256 "__SHA256__"

  url "https://github.com/abhi-singhs/agent-voice-app/releases/download/v#{version}/Agent.Voice.App_#{version}_aarch64.dmg"
  name "Agent Voice App"
  desc "Desktop phone for your coding agent (spoken back-and-forth over MCP)"
  homepage "https://github.com/abhi-singhs/agent-voice-app"

  livecheck do
    url :url
    strategy :github_latest
  end

  # Release builds are unsigned (no Apple Developer ID / notarization), and only
  # an Apple Silicon DMG is published today.
  depends_on arch: :arm64
  depends_on macos: :catalina

  app "Agent Voice App.app"

  # The DMG is unsigned, so macOS quarantines the browser/download copy and shows
  # "damaged and can't be opened". Strip the quarantine flag so it launches
  # cleanly. Drop this once the app is signed + notarized.
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", "#{appdir}/Agent Voice App.app"]
  end

  uninstall quit: "in.abhisingh.agent-voice-app"

  zap trash: [
    "~/.agent-voice-app",
    "~/Library/Caches/in.abhisingh.agent-voice-app",
    "~/Library/Preferences/in.abhisingh.agent-voice-app.plist",
    "~/Library/Saved Application State/in.abhisingh.agent-voice-app.savedState",
    "~/Library/WebKit/in.abhisingh.agent-voice-app",
  ]
end
