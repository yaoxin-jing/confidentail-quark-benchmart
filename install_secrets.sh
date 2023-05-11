#!/bin/bash
set -e 
# example of using arguments to a script

pushd secret

mkdir -p mongo_resource
mkdir -p /opt/confidential-containers/kbs/repository/quark_mongo

mkdir -p nginx_resource
mkdir -p /opt/confidential-containers/kbs/repository/quark_nginx

pushd mongo


for filename in *; do
cat $filename | base64 | tr -d '\n' > ../mongo_resource/$filename
done

cp -R ../mongo_resource  /opt/confidential-containers/kbs/repository/quark_mongo
popd

pushd nginx

for filename in *; do
cat $filename | base64 | tr -d '\n' > ../nginx_resource/$filename
done
cp -R ../nginx_resource  /opt/confidential-containers/kbs/repository/quark_mongo
popd


popd


