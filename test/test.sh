#!/bin/bash

echo "test script started.."

set -e

# Exit status
STATUS=0

echo "test script started.."

# Launches the blockchain demo and waits until it starts listening
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
    cd test
}

# Kills whatever program is listening on the TCP port 8000, on which the cryptocurrency
# demo needs to bind to.
function kill-server {
    SERVER_PID=`lsof -iTCP -sTCP:LISTEN -n -P 2>/dev/null |  awk '{ if ($9 == "*:8000") { print $2 } }'`
    if [[ -n $SERVER_PID ]]; then
        kill -9 $SERVER_PID
    fi
}

# make a bet in the exchange.
#
# Arguments:
# - $1: filename with the transaction data
function make-bet {
    RESP=`curl -H "Content-Type: application/json" -X POST -d @$1 http://127.0.0.1:8000/api/services/exchange/v1/bets 2>/dev/null`
}

function order {
    RESP=`curl -H "Content-Type: application/json" -X POST -d @$1 http://127.0.0.1:8000/api/services/exchange/v1/order 2>/dev/null`
}

kill-server
launch-server

echo "make order for Serg..."
order order2.json

echo "make first bet for Serg..."
make-bet make-bet.json

echo "Waiting until transactions are committed..."
sleep 7


kill-server
exit $STATUS
