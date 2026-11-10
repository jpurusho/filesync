# FileSync - Project Archived

**Status:** 🪦 **Archived** - No longer under active development  
**Date Archived:** 2026-11-10  
**Final Version:** 0.3.8

---

## Why This Project Was Archived

FileSync was an experimental peer-to-peer file synchronization application for macOS. After extensive development (100+ hours) and real-world testing, we discovered **fundamental limitations** that make it impractical for general use:

### Critical Issues Discovered

1. **VPN Incompatibility**
   - Corporate and personal VPNs block peer-to-peer connections
   - Most professionals cannot use FileSync while on VPN
   - No practical workaround without relay servers (defeats "no cloud" goal)

2. **Auto-Discovery Failures**
   - mDNS/Bonjour discovery unreliable in practice
   - Works in lab, fails on real networks
   - Users forced to manually enter IP addresses

3. **LAN-Only Limitation**
   - Both computers must be on same local network
   - Cannot sync remotely (home ↔ work) without both computers present
   - Limits primary use case for distributed teams

4. **Complexity vs. Value**
   - 30+ minutes of troubleshooting vs. cloud sync's 2 minutes
   - Requires: disable VPN, configure firewall, manual pairing
   - Unacceptable UX for non-technical users

5. **Better Alternatives Already Exist**
   - **Syncthing:** Proven open-source P2P sync (works over internet with relays)
   - **Nextcloud:** Self-hosted cloud (privacy + reliability)
   - **Cloud sync:** Google Drive/Dropbox (just works)

### Strategic Assessment

A comprehensive analysis found:
- **Market size:** ~5,000 potential users worldwide (too small)
- **Additional development needed:** 200-300 hours to reach "usable" state
- **Engineering cost:** $20,000-30,000 in time (at $100/hour)
- **vs. Cloud cost:** $120/year (breaks even never)

**See:** [docs/STRATEGIC-ASSESSMENT.md](docs/STRATEGIC-ASSESSMENT.md) for full analysis

---

## What Was Built

Despite not being viable as a product, FileSync achieved its **technical goals**:

### ✅ Completed Features (M0-M7)

- ✅ **Core sync engine:** Push, Pull, Bidirectional with conflict resolution
- ✅ **Security:** TLS 1.3 with TOFU certificate pinning
- ✅ **Network layer:** mDNS discovery, peer pairing, encrypted transfer
- ✅ **Profile management:** Full CRUD with multi-anchor support
- ✅ **State management:** SQLite with WAL mode, sync index tracking
- ✅ **Conflict resolution:** Newer-wins and keep-both policies
- ✅ **Profile replication:** Configuration syncs between peers
- ✅ **Real-time progress:** File-by-file progress with cancellation
- ✅ **Tauri UI:** Modern React interface with dark theme
- ✅ **Test coverage:** 92 tests across sync engine, network, profiles
- ✅ **Documentation:** 27 ADRs, comprehensive README, milestone plans

### 📊 Final Statistics

- **Lines of Code:** 9,001 (Rust) + 1,751 (TypeScript/React)
- **Test Coverage:** 92 tests (37 sync, 21 network, 9 profile, 25 utils)
- **Architecture Decisions:** 27 ADRs documented
- **Time Invested:** ~100-120 hours (MVP development)
- **Milestones Completed:** M0 through M7 (all planned milestones)

### 🏆 Technical Achievements

**What worked well:**
- Clean architecture (4 Rust crates + platform layer)
- Proper security (TLS, certificate pinning, path traversal protection)
- Solid sync engine (reconciliation, conflict detection, atomic writes)
- Comprehensive testing (unit, integration, acceptance scenarios)
- Professional documentation (ADRs, specs, plans)

**Engineering quality:** A  
**Product viability:** F

---

## Alternative Solutions

### For Privacy-Focused Users

**1. Self-Hosted Nextcloud** (Recommended)
- Privacy: Files on your hardware
- Reliability: Works on VPN, remotely, mobile
- Setup: Weekend project (~4 hours)
- Cost: $500-700 hardware one-time
- **See:** [nextcloud-setup](https://github.com/jpurusho/nextcloud-setup) for our implementation

**2. Syncthing** (Quick Alternative)
- Open-source P2P sync
- Works over internet (relay servers)
- Cross-platform (Mac, Windows, Linux, Android)
- Setup: 30 minutes
- Cost: Free
- Install: `brew install syncthing`

### For Ease-of-Use Priority

**3. Cloud Sync** (Google Drive, Dropbox)
- Setup: 5 minutes
- Maintenance: Zero
- Cost: $10-20/month
- Works: Everywhere, always, with mobile

---

## Lessons Learned

### What We Learned About P2P File Sync

1. **Network complexity is underestimated**
   - VPNs, firewalls, NAT traversal, discovery
   - Every network is different
   - "Works on my network" ≠ works everywhere

2. **UX matters more than engineering**
   - Clean code doesn't matter if users can't pair devices
   - 30 minutes of troubleshooting > 2-minute cloud signup
   - Technical users will use rsync; non-technical won't troubleshoot

3. **Existing solutions exist for a reason**
   - Syncthing has solved these problems (relay servers, cross-platform)
   - Reinventing the wheel is expensive
   - Open-source collaboration > solo development

4. **P2P requires always-on infrastructure**
   - Discovery requires mDNS (fragile) or discovery servers (cloud)
   - NAT traversal requires STUN/TURN servers (cloud)
   - Relay for VPN/remote requires relay servers (cloud)
   - **Pure P2P without any servers is impractical**

5. **Privacy vs. Convenience trade-off is real**
   - Cloud sync is more convenient because it uses servers (intermediary)
   - P2P sync is more private but requires both peers online + direct connection
   - Self-hosted cloud (Nextcloud) is the middle ground

### Technical Insights

**What worked:**
- Rust for core logic (safe, fast, correct)
- SQLite for state management (reliable, transactional)
- TOFU for trust model (simple, secure-enough)
- Tauri for native UI (small binaries, modern web stack)

**What didn't:**
- mDNS for discovery (blocked by VPNs, firewalls, enterprise networks)
- LAN-only P2P (too limiting for real use)
- Manual pairing (friction kills adoption)
- macOS-only (limits market)

---

## Project History

### Milestones

- **M0 (2026-06-12):** Scaffolding - Tauri + React, crate structure
- **M1 (2026-06-13):** Local sync engine - scan, diff, reconcile, apply
- **M2 (2026-06-14):** Discovery & pairing - mDNS, TOFU TLS
- **M3 (2026-06-15):** Networked transfer - push/pull over TLS
- **M3.5 (2026-06-15):** Quick-send - profile-less one-shot transfer
- **M4 (2026-06-16):** Bidirectional reconciliation + clock-skew handling
- **M5 (2026-06-17):** Profile replication - share configs, tombstones
- **M6 (2026-06-18):** Tauri UI - profiles editor, peers, sync controls
- **M7 (2026-06-24):** MVP readiness - integration, tests, docs
- **Testing (2026-11-09):** Two-Mac testing - discovered critical issues
- **Analysis (2026-11-10):** Strategic assessment - decision to archive

### Key Decision Points

1. **ADR-0019:** Discovered Db + async blocker (resolved with Arc<Db>)
2. **ADR-0027:** Added macOS entitlements (didn't solve discovery)
3. **Testing Phase:** VPN blocking, mDNS failures (fatal for adoption)
4. **Strategic Assessment:** Analysis showed <5,000 user market, 200+ hours more needed

---

## For Developers

### If You Want to Learn From This Code

**The codebase demonstrates:**

- ✅ **Clean Rust architecture** - multiple crates, clean boundaries
- ✅ **Proper testing** - unit, integration, property-based tests
- ✅ **Security patterns** - TLS, certificate pinning, path validation
- ✅ **State management** - SQLite transactions, sync index design
- ✅ **Tauri integration** - Rust backend + React frontend
- ✅ **Documentation** - ADRs, specs, comprehensive README

**Useful for studying:**
- Conflict resolution algorithms (`crates/synccore/src/reconcile.rs`)
- TLS/TOFU implementation (`crates/syncnet/src/pairing.rs`)
- SQLite with Rust (`crates/syncstore/src/`)
- mDNS discovery (`crates/syncnet/src/discovery.rs`)
- Tauri command patterns (`src-tauri/src/commands.rs`)

### Running the Code

**Build from source:**
```bash
git clone https://github.com/jpurusho/filesync.git
cd filesync
make dev  # Development mode
# or
make build  # Production build
```

**Run tests:**
```bash
make test  # All 92 tests
```

**Note:** Auto-discovery likely won't work on your network (see issues above).

---

## Acknowledgments

Built with:
- [Tauri](https://tauri.app/) - Rust + web UI framework
- [Tokio](https://tokio.rs/) - Async runtime
- [Rustls](https://github.com/rustls/rustls) - TLS implementation
- [rusqlite](https://github.com/rusqlite/rusqlite) - SQLite bindings
- [React](https://react.dev/) + [Zustand](https://zustand-demo.pmnd.rs/) - UI
- [mdns-sd](https://crates.io/crates/mdns-sd) - mDNS discovery

Developed with assistance from Claude (Anthropic).

---

## License

MIT License - See [LICENSE](LICENSE) file

---

## Contact

For questions about the code or architecture decisions:
- Email: jerome.purushotham@gmail.com
- GitHub: [@jpurusho](https://github.com/jpurusho)

---

**This project taught us a lot about P2P networking, distributed systems, and the importance of validating product-market fit before deep implementation. We're now building [nextcloud-setup](https://github.com/jpurusho/nextcloud-setup) - a better solution to the same problem.**

🪦 *RIP FileSync (2026-06-12 to 2026-11-10) - You were technically impressive but practically impossible.*
