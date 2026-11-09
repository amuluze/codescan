const app = createServer();
const router = createRouter();

// ruleid: ts/example-unauthed-write-route
app.post("/users", createUser);

// ruleid: ts/example-unauthed-write-route
router.put("/users/:id", updateUser);

// ruleid: ts/example-unauthed-write-route
app.delete("/users/:id", removeUser);

app.post("/users", requireAuth, createUser);
app.get("/users", listUsers);
router.post(usersPath, createUser);
