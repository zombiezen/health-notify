# Copyright 2024 Ross Light
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     https://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
#
# SPDX-License-Identifier: Apache-2.0

{
  inputs = {
    nixpkgs.url = "nixpkgs";
    flake-utils.url = "flake-utils";
    stamp.url = "github:zombiezen/stamp.nix";
  };

  outputs = { self, nixpkgs, flake-utils, stamp, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };

        health-notify = pkgs.callPackage ./package.nix {};
      in
      {
        packages.default = self.packages.${system}.health-notify;
        packages.health-notify = stamp.lib.stamp {
          inherit pkgs;
          sourceInfo = self.sourceInfo;
          wrapped = health-notify;
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ health-notify ];

          packages = [
            pkgs.cargo
            pkgs.rust-analyzer
            pkgs.rustfmt
          ];
        };
      }
    );
}
