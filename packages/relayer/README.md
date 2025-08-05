# Relayer Packages

The packages in this directory are used to build components of the IBC Eureka relayer. Each package serves a specific purpose in the relayer's architecture. They are put together to build a modular and extensible relayer binary in the [programs directory](../../programs/relayer/).

The packages are organized as follows:
- `core/`: Contains the core traits and types used to build the relayer binary and modules.
- `lib/`: Contains the shared libraries used to build relayer modules.
- `modules/`: Contains modules which are essentially one sided relayers between two chain types. They are invoked by the relayer binary to perform relayer operations.
