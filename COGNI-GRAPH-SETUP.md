# CogniGraph Neo4j Setup Guide

## Quick Setup Options

### Option 1: Docker (Recommended)
```bash
# Run Neo4j with default credentials
docker run \
  --name neo4j-cogni \
  --publish=7474:7474 \
  --publish=7687:7687 \
  --env=NEO4J_AUTH=neo4j/password \
  neo4j:latest
```

Then use these settings in the CogniGraph interface:
- **URI**: `bolt://localhost:7687`
- **Username**: `neo4j`
- **Password**: `password`

### Option 2: Neo4j Desktop
1. Download Neo4j Desktop: https://neo4j.com/download/
2. Create a new project and database
3. Set password during setup
4. Start the database
5. Use the connection details shown in Neo4j Desktop

### Option 3: Neo4j Community Edition
1. Download from: https://neo4j.com/download-center/#community
2. Install and configure
3. Set initial password with: `neo4j-admin set-initial-password yourpassword`
4. Start Neo4j service

## Connection Configuration

In the CogniGraph interface, update the connection settings:

```
URI: bolt://localhost:7687
Username: neo4j  
Password: [your-password]
```

## Troubleshooting

### Authentication Failure
- Ensure Neo4j is running
- Verify username/password are correct
- Check if you need to change the default password

### Connection Refused  
- Check Neo4j is running on port 7687
- Verify firewall settings
- Ensure bolt connector is enabled

### Version Compatibility
- Neo4j 4.4+ recommended
- Neo4j 5.11+ for vector search features
- Community Edition is sufficient for basic features

## Testing Connection

1. Open the CogniGraph interface
2. Enter your connection details
3. Click "测试连接" (Test Connection)
4. If successful, click "初始化图谱" (Initialize Graph)

## Default Credentials

If using Docker or fresh installation:
- Username: `neo4j`
- Password: `password` (or your chosen password)