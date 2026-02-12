# Wallet RPC Example 

1. Install bitcoind
2. Start bitcoind in regtest mode.
   ```
   just start
   ```
3. Create test bitcoind wallet and generate regtest blocks.
   ```
   just create
   just generate 110 $(just address)
   ```
4. Run the example and note the wallet's address and balance.
   ```
   just run
   ```
5. Send regtest coins to the wallet address.
   ```
   just send 10 <wallet address>
   just generate 6 $(just address)
   ```
6. Re-run example and note the new balance.
   ```
   just run 
   ```
7. Stop the regtest bitcoind.
   ``` 
   just stop
   ```
8. Cleanup test data (optional).
   ``` 
   just clean
   ```

