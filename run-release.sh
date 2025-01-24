set -e
dbsdk-cli build -p release .
EXEC_PATH=$(readlink -f ./build/release.iso)
echo $EXEC_PATH
pushd ~/.config/itch/apps/dreambox/ && ./dreambox -f "$EXEC_PATH" && popd