set -ex

curl -O https://packages.couchbase.com/clients/c/libcouchbase-3.0.0_debian10_buster_amd64.tar
tar xf libcouchbase-3.0.0_debian10_buster_amd64.tar
cd libcouchbase-3.0.0_debian10_buster_amd64
apt install libevent-core-2.1-6
dpkg -i libcouchbase3{-tools,-libevent,}_3.0.0*.deb libcouchbase-dev*.deb