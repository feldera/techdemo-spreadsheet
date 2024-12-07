FILE_PATH="/tmp/spreadsheet_data.parquet"
BUCKET_NAME="spreadsheet-backups"

curl -L "${FELDERA_HOST}/v0/pipelines/xls/query?sql=SELECT%20*%20FROM%20spreadsheet_data%3B&format=parquet" > $FILE_PATH
TIMESTAMP=$(date -u +"%Y%m%d%H%M%S")

# Extract the base filename and append the timestamp
BASE_FILENAME=$(basename "$FILE_PATH")
RENAMED_FILENAME="${BASE_FILENAME/spreadsheet_data/spreadsheet_data_$TIMESTAMP}"
S3_KEY="$RENAMED_FILENAME"

# Check if the file exists
if [[ ! -f "$FILE_PATH" ]]; then
  echo "Error: File $FILE_PATH does not exist."
  exit 1
fi

# Upload the file to S3
echo "Uploading $FILE_PATH as s3://$BUCKET_NAME/$S3_KEY"
aws s3 cp "$FILE_PATH" "s3://$BUCKET_NAME/$S3_KEY"

# Verify the upload
if [[ $? -eq 0 ]]; then
  echo "File uploaded successfully to s3://$BUCKET_NAME/$S3_KEY"
else
  echo "Error: Upload failed."
  exit 1
fi