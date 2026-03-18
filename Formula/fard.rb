class Fard < Formula
  desc "Deterministic, content-addressed scripting language with cryptographic receipts"
  homepage "https://github.com/mauludsadiq/FARD"
  version "1.6.0"
  license "MUI"

  on_macos do
    on_arm do
      url "https://github.com/mauludsadiq/FARD/releases/download/v1.6.0/fard-macos-aarch64.tar.gz"
      sha256 :no_check
    end
    on_intel do
      url "https://github.com/mauludsadiq/FARD/releases/download/v1.6.0/fard-macos-x86_64.tar.gz"
      sha256 :no_check
    end
  end

  on_linux do
    url "https://github.com/mauludsadiq/FARD/releases/download/v1.6.0/fard-linux-x86_64.tar.gz"
    sha256 :no_check
  end

  BINARIES = %w[
    fardrun fardfmt fardcheck fardwasm fardregistry
    fardlock fardbundle fardverify fardpkg fardc farddoc fard-build
  ].freeze

  def install
    BINARIES.each do |b|
      bin.install b if File.exist?(b)
    end
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/fardrun --version 2>&1")
  end
end
