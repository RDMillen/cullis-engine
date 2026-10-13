{
  description = "CullisEngine development shell";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      # Loaded at runtime by wgpu and winit.
      runtimeLibs = with pkgs; [
        vulkan-loader
        libxkbcommon
        wayland
        xorg.libX11
        xorg.libXcursor
        xorg.libXi
        xorg.libXrandr
      ];
    in {
      devShells.${system}.default = pkgs.mkShell {
        # Rust itself comes from rustup, pinned by rust-toolchain.toml.
        packages = with pkgs; [
          rustup
          pkg-config
          vulkan-tools
          mesa # lavapipe software Vulkan for the VM
          ffmpeg # cinematic conversion in fable-convert
          python3
          alsa-lib # cpal (sound output)
        ] ++ runtimeLibs;

        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
        VK_ICD_FILENAMES = "${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.x86_64.json";
      };
    };
}
