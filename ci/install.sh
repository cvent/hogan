set -ex

main() {
    local target=
    if [ $TRAVIS_OS_NAME = linux ]; then
        target=x86_64-unknown-linux-musl
        sort=sort      
    else
        target=x86_64-apple-darwin
        sort=gsort  # for `sort --sort-version`, from brew's coreutils.
        #brew install openssl@1.1
        #cargo clean
    fi

    # Builds for iOS are done on OSX, but require the specific target to be
    # installed.
    case $TARGET in
        aarch64-apple-ios)
            rustup target install aarch64-apple-ios
            ;;
        armv7-apple-ios)
            rustup target install armv7-apple-ios
            ;;
        armv7s-apple-ios)
            rustup target install armv7s-apple-ios
            ;;
        i386-apple-ios)
            rustup target install i386-apple-ios
            ;;
        x86_64-apple-ios)
            rustup target install x86_64-apple-ios
            ;;
        x86_64-unknown-linux-musl)    
            #build custom target images
            docker build -t hogan-x86_64-unknown-linux-musl -f ci/docker/Dockerfile.musl ci/docker
            ;;
        x86_64-unknown-linux-gnu)
            docker build -t hogan-x86_64-unknown-linux-gnu -f ci/docker/Dockerfile.gnu ci/docker
            ;;
    esac
    cargo install cross
}

main
