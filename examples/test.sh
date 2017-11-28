#!/bin/bash

set -e

# Exit status
STATUS=0

# Launches the exchange demo and waits until it starts listening
# on the TCP port 8000.
function launch-server {
    cd ..
    cargo run &
    CTR=0
    MAXCTR=60
    while [[ ( -z `lsof -iTCP -sTCP:LISTEN -n -P 2>/dev/null |  awk '{ if ($9 == "*:8000") { print $2 } }'` ) && ( $CTR -lt $MAXCTR ) ]]; do
      sleep 1
      CTR=$(( $CTR + 1 ))
    done
    if [[ $CTR == $MAXCTR ]]; then
        echo "Failed to launch the server; aborting"
        exit 1
    fi
    cd examples
}

# Kills whatever program is listening on the TCP port 8000, on which the cryptocurrency
# demo needs to bind to.
function kill-server {
    SERVER_PID=`lsof -iTCP -sTCP:LISTEN -n -P 2>/dev/null |  awk '{ if ($9 == "*:8000") { print $2 } }'`
    if [[ -n $SERVER_PID ]]; then
        kill -9 $SERVER_PID
    fi
}


function order {
    RESP=`curl -H "Content-Type: application/json" -X POST -d @$1 http://127.0.0.1:8000/api/services/excange/v1/order 2>/dev/null`
}


# Checks a response to an Exonum transaction.
#
# Arguments:
# - $1: expected start of the transaction hash returned by the server
function check-transaction {
    if [[ `echo $RESP | jq .tx_hash` =~ ^\"$1 ]]; then
        echo "OK, got expected transaction hash $1"
    else
        echo "Unexpected response: $RESP"
        STATUS=1
    fi
}

kill-server
launch-server

echo "make all orders"
order order1.json
order order2.json
order order3.json
order order4.json
order order5.json


echo "Waiting until transactions are committed..."
sleep 7



kill-server
exit $STATUS
