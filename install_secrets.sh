#!/bin/bash
set -e 
# example of using arguments to a script

pushd secret

mkdir -p mongo_resource
mkdir -p /opt/confidential-containers/kbs/repository/quark_mongo

mkdir -p nginx_resource
mkdir -p /opt/confidential-containers/kbs/repository/quark_nginx

mkdir -p file_resource
mkdir -p /opt/confidential-containers/kbs/repository/default/files

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



pushd file_secrets

for filename in *; do
cat $filename | base64 | tr -d '\n' > ../file_resource/$filename
done
cp -R ../file_resource/*  /opt/confidential-containers/kbs/repository/default/files
popd


popd


