set -e
dbsdk-cli build .
EXEC_PATH=$(readlink -f ./build/debug.iso)
echo $EXEC_PATH
pushd ~/.config/itch/apps/dreambox/ && ./dreambox -f "$EXEC_PATH" && popd