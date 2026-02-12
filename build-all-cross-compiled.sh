for target in $(cat package.json | jq -r .napi.targets[])
do
  echo "-----------------------------------------------"
  echo "|  compiling for $target"
  echo "-----------------------------------------------"
  npm run build -- --cross-compile --target $target || exit $?
  echo
done