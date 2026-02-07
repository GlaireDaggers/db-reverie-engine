set -e
dbsdk-cli build .
EXEC_PATH=$(readlink -f ./build/debug.iso)
echo $EXEC_PATH
pushd ~/.config/itch/apps/dreambox/ && ./DreamboxVM -b -s "$EXEC_PATH" && popd