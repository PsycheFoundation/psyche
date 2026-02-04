#!/usr/bin/env sh

set -eu

test_nix_installation () {
    nix --version 1> /dev/null 2> /dev/null
}

install_nix () {
    TMP=$(mktemp -d)

    curl --proto '=https' --tlsv1.2 -sSfL https://install.determinate.systems/nix -o "$TMP/nix-installer.sh"
    chmod u+x "$TMP/nix-installer.sh"

    "$TMP/nix-installer.sh" \
        install \
        --extra-conf "extra-substituters = https://cache.garnix.io https://cache.nixos-cuda.org" \
        --extra-conf "extra-trusted-public-keys = cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g= cache.nixos-cuda.org:74DUi4Ye579gUqzH4ziL9IyiJBlDpMRn9MBN8oNan9M=" \
        --diagnostic-endpoint=""

    set +eu
    # shellcheck source=/dev/null
    . /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
    set -eu
}

test_cache_configured () {
    nixConfig=$(nix --extra-experimental-features 'nix-command flakes' show-config) &&
    (echo "$nixConfig" | grep --quiet "^substituters.*cache\.garnix\.io") &&
    (echo "$nixConfig" | grep --quiet "^trusted-public-keys.*cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g=") &&
    true
}

configure_cache () {
    echo extra-substituters = https://cache.garnix.io | sudo tee -a /etc/nix/nix.conf > /dev/null
    echo extra-trusted-public-keys = cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g= | sudo tee -a /etc/nix/nix.conf > /dev/null

    echo trying to restart the nix-daemon...
    # This ignores errors, for single-user installations.
    if test "$(uname)" = "Linux" ; then
        sudo systemctl restart nix-daemon.service || true
    elif test "$(uname)" = "Darwin" ; then
        sudo launchctl kickstart -k system/org.nixos.nix-daemon || true
    else
        echo "Unknown system: $(uname)"
        echo Cancelling installation.
        exit 1
    fi
}

echo
echo NIX INSTALLATION
echo ================

if test_nix_installation; then
    echo Hooray, nix is already installed:
    nix --version
else
    cat << EOF
Welcome to the 'psyche' dev env installer!

'psyche' depends on nix, but it seems that you don't have a nix
installation. This installer will install nix for you. It uses the
'nix-installer' by Determinate Systems. For more information, see
https://github.com/DeterminateSystems/nix-installer.

This installer will ask for your admin password to install nix.

If in the future you want to uninstall Nix, use:

$ /nix/nix-installer uninstall

EOF

    printf "Should I install nix now? [y/n] "
    read -r SHOULD_INSTALL_NIX
    if test "$SHOULD_INSTALL_NIX" != y; then
        echo Cancelling installation.
        exit 1
    fi
    install_nix
    test_nix_installation || (echo "Failed to install nix, cancelling installation." && exit 1)
    echo nix is now installed:
    nix --version
fi

echo
echo BINARY CACHE CONFIGURATION
echo ==========================

if test_cache_configured; then
    echo Hooray, the garnix cache is configured.
elif true >> /etc/nix/nix.conf; then
    echo For \'psyche\' to work well, nix needs to use the garnix.io binary
    echo cache. But the cache configuration cannot be found. This
    echo installer will modify /etc/nix/nix.conf to add the garnix.io binary cache.
    echo
    printf "Should I add the garnix.io binary cache to /etc/nix/nix.conf now (requires sudo)? [y/n] "
    read -r SHOULD_CONFIGURE_CACHE
    if test "$SHOULD_CONFIGURE_CACHE" != y; then
        echo Cancelling installation.
        exit 1
    fi
    configure_cache
    test_cache_configured || (echo "Failed to configure the garnix cache, cancelling installation." && exit 1)
    echo The garnix cache is now configured.
else
    echo "For 'psyche' to work well, Nix needs to use the garnix.io binary"
    echo "cache. But Nix currently isn't configured to use that cache."
    echo
    echo "This installer attempted to add the cache config to /etc/nix/nix.conf,"
    echo "but that file was not writable (you may be using NixOS)."
    echo "Please add the following lines to your Nix config and restart your nix-daemon:"
    echo "    extra-substituters = https://cache.garnix.io"
    echo "    extra-trusted-public-keys = cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g="
    echo
    echo "On non-NixOS systems you can add these lines to /etc/nix/nix.conf."
    echo "On NixOS you can add these to nix.extraOptions in /etc/nixos/configuration.nix."
    echo
    echo "Once you have added these lines please re-run this installer."
    exit 1
fi

echo "Success!"
