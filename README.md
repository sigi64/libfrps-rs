# libfrps-rs

Is Rust implementation of My previous Employer binary serialization protocol (see https://seznam.github.io/frpc/ ). It was based on another (not public) C++ implementatiton we did there. It is *non-allocating push parser* (see https://stackoverflow.com/questions/15895124/what-is-push-approach-and-pull-approach-to-parsing) implemented with manually encoded final state automata. It is tested with same test suite data like we used in production. 

This project I intended for Rust demostration and was completly written in my spare time after working hours. If you are starting a new project which is not related to Seznam.cz infrastructure I think there are now better alternatives like Google Protobuf. Fastrpc is more than 5 years older than gRPC and not all protocol specification are well done.

