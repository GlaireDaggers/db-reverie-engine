set -e
dbsdk-cli build -p release .
EXEC_PATH=$(readlink -f ./build/release.iso)
echo $EXEC_PATH
pushd ~/.config/itch/apps/dreambox/ && ./DreamboxVM -b -s "$EXEC_PATH" && popd
