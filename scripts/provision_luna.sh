#!/usr/bin/env bash
set -euo pipefail

# Historical versions of this script installed wildcard LS2 roles and "all"
# client permissions for the capture service, as well as impersonation-capable
# names. Those changes are unsuitable for the private, group-gated C5 API.
#
# Never mutate the TV's permission databases as part of capture investigation.
# The historical implementation remains available in Git history for audit,
# but must not run implicitly through make provision-luna.

TV_IP="${1:-local}"

cat >&2 <<EOF
REFUSE: legacy Luna wildcard provisioning is disabled.

Target: ${TV_IP}
Reason: the LG C5 capture methods belong to the private capture.client
group, while checkPermission/setPermission belong to the private
capturepermission.security group. A root-shell getCapability response is
not evidence that the Rust daemon may capture any frames.

This command has made no changes to manifests, roles, client permissions,
services or capture settings.

For read-only policy inspection see:
  docs/C5-IMAGE-STACK-OFFLINE-ANALYSIS-20261017.md
  scripts/c5-readonly-improvement-inventory.sh

If existing broad roles were installed previously, audit them individually;
do not overwrite, delete, or rescan the active permission database blindly.
EOF
exit 2
