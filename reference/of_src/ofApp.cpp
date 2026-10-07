#include "ofApp.h"

//--------------------------------------------------------------
void ofApp::setup(){
    loadConfig();
    sender.setup(distIp, distPort);
    rev.setup(listenPort);
    gui.setup();
    ofSetFrameRate(60);
}

//--------------------------------------------------------------
void ofApp::update(){
    while(rev.hasWaitingMessages()){
        ofxOscMessage msg;
        if(rev.getNextMessage(msg)){
            if(msg.getAddress()=="/hokuyohub/cluster"){
                uint64_t timeStamp=msg.getArgAsInt64(1);
                if(!footHistory.empty()&&footHistory.rbegin()->first>timeStamp){
                    footHistory.clear();
                    footStampHistory.clear();
                }//ループしたら配列をクリアする
                Foot f;
                f.pos=glm::vec2(msg.getArgAsFloat(3),msg.getArgAsFloat(4));
                f.timeStamp=timeStamp;

                //連続とみなすひとつ前の足の住所を探す
                f.prevFootAddress=findNearestInWindow(footHistory,f.pos,timeStamp,matchWindow,0,matchMaxDist);
                
                //連続する足が見つかった場合は速度を計算する
                //速度の推移から着地とみなすかどうかを判定する
                if(f.prevFootAddress.first!=0){
                    auto & prevFoot=footHistory[f.prevFootAddress.first][f.prevFootAddress.second];
                    const Foot & velBaseFoot = findOldestLinkedFoot(f, matchWindow);
                    if(timeStamp-velBaseFoot.timeStamp>0){
                        f.vel=(f.pos-velBaseFoot.pos)/((timeStamp-velBaseFoot.timeStamp)*1e-9);
                    }else{
                        f.vel=glm::vec2(0,0);
                    }

                    if(glm::length2(f.vel)<landingSpeed*landingSpeed){
                        //着地とみなし、blockerの範囲外なら足跡スタンプを設置する
                        bool isBlocked=false;
                        for(auto & b:blockers){
                            if(glm::length2(f.pos-b.pos)<blockerRadius*blockerRadius){
                                isBlocked=true;
                            }
                        }
                        if(!isBlocked){
                            FootStamp fs;
                            fs.pos=f.pos;
                            //向きを設定するために過去の足の履歴を参照する

                            const Foot & dirBaseFoot = findOldestLinkedFoot(f, directionWindow);                            
                            fs.dir=fs.pos-dirBaseFoot.pos;
                            if(glm::length2(fs.dir)>0.0001){
                                fs.dir=glm::normalize(fs.dir);
                            }else{//一旦向きが決まっていない場合は上向きにする
                                fs.dir=glm::vec2(0,1);
                            }

                            fs.prevStampAddress=findNearestInWindow(footStampHistory,fs.pos,timeStamp,prevStampWindow,prevStampMinDist,prevStampMaxDist);

                            if(fs.prevStampAddress.first!=0){
                                auto & pred = footStampHistory[fs.prevStampAddress.first][fs.prevStampAddress.second];
                                glm::vec2 h = fs.dir+pred.dir;
                                if(glm::length2(h)>1){
                                    glm::vec2 toPred = pred.pos - fs.pos;       // 今の足跡 → 直前の足跡
                                    float lat = (h.x*toPred.y - h.y*toPred.x) / glm::length(h);   // 符号付きの横ずれ[m]

                                    
                                    if(fabs(lat) >= 0.1 && fabs(lat) <= 0.4){
                                        if(lat<0){
                                            fs.side=LEFT;
                                        }
                                        else{
                                            fs.side=RIGHT;
                                        }
                                    }
                                    else{
                                        fs.side=-pred.side;
                                    }
                                }
                            }
                            sendFootStampOSC(fs);
                            footStampHistory[timeStamp].push_back(fs);

                            Blocker b;
                            b.pos=fs.pos;
                            b.lastSeen=timeStamp;
                            blockers.push_back(b);
                        }
                    }
                }
                
                footHistory[timeStamp].push_back(f);
            }
        }
    }

    //blockerの寿命をチェックして古いものを削除する
    if(!footHistory.empty()){
        uint64_t now=footHistory.rbegin()->first;
        for(auto bit=blockers.begin();bit!=blockers.end();){
            //blockerの範囲内に足があればlastSeenを更新する
            for(auto & f:footHistory[now]){
                if(glm::length2(f.pos-bit->pos)<blockerRadius*blockerRadius){
                    bit->lastSeen=now;
                    break;
                }
            }
            if(now-bit->lastSeen>blockerLifeTime*1e9){
                bit=blockers.erase(bit);
            }else{
                ++bit;
            }
        }
    }
    
    checkFootLife();
    checkFootStampLife();
}

void ofApp::checkFootLife(){
    if(footHistory.empty())return;
    uint64_t now=footHistory.rbegin()->first;
    for(auto it=footHistory.begin();it!=footHistory.end();){
        if(now-it->first>lifeTime*1e9){
            it=footHistory.erase(it);
        }else{
            break; 
        }
    }
}

void ofApp::checkFootStampLife(){
    if(footStampHistory.empty())return;
    uint64_t now=footStampHistory.rbegin()->first;
    for(auto it=footStampHistory.begin();it!=footStampHistory.end();){
        if(now-it->first>stampLifeTime*1e9){
            it=footStampHistory.erase(it);
        }else{
            break;
        }
    }
}

void ofApp::saveConfig(){
    ofJson json;
    json["lifeTime"]=lifeTime;
    json["matchWindow"]=matchWindow;
    json["matchMaxDist"]=matchMaxDist;
    json["landingSpeed"]=landingSpeed;
    json["directionWindow"]=directionWindow;
    json["stampLifeTime"]=stampLifeTime;
    json["prevStampWindow"]=prevStampWindow;
    json["prevStampMinDist"]=prevStampMinDist;
    json["prevStampMaxDist"]=prevStampMaxDist;
    json["blockerRadius"]=blockerRadius;
    json["blockerLifeTime"]=blockerLifeTime;
    json["distIp"]=distIp;
    json["distPort"]=distPort;
    json["listenPort"]=listenPort;
    ofSavePrettyJson("config.json",json);
}

void ofApp::loadConfig(){
    if(!ofFile::doesFileExist("config.json"))return;
    ofJson json=ofLoadJson("config.json");
    //キーがない場合は今の値のまま
    lifeTime=json.value("lifeTime",lifeTime);
    matchWindow=json.value("matchWindow",matchWindow);
    matchMaxDist=json.value("matchMaxDist",matchMaxDist);
    landingSpeed=json.value("landingSpeed",landingSpeed);
    directionWindow=json.value("directionWindow",directionWindow);
    stampLifeTime=json.value("stampLifeTime",stampLifeTime);
    prevStampWindow=json.value("prevStampWindow",prevStampWindow);
    prevStampMinDist=json.value("prevStampMinDist",prevStampMinDist);
    prevStampMaxDist=json.value("prevStampMaxDist",prevStampMaxDist);
    blockerRadius=json.value("blockerRadius",blockerRadius);
    blockerLifeTime=json.value("blockerLifeTime",blockerLifeTime);
    distIp=json.value("distIp",distIp);
    distPort=json.value("distPort",distPort);
    listenPort=json.value("listenPort",listenPort);
}

//--------------------------------------------------------------
void ofApp::draw(){
    

    ofPushMatrix();
    ofTranslate(50, 50);
    ofPushStyle();
    ofScale(100,-100);
    // ofScale(2);
    ofNoFill();
    ofDrawRectangle(0,0,5.4,-5.4);

    //blockerを描画
    ofFill();
    for(auto & b:blockers){
        ofSetColor(0,0,255,50);
        ofDrawCircle(b.pos,blockerRadius);
    }

    //足跡スタンプを描画
    ofFill();
    if(footStampHistory.size()>0){
        uint64_t now=footStampHistory.rbegin()->first;
        for(auto & fh:footStampHistory){
            for(auto & f:fh.second){
                float alpha=ofMap(now-fh.first,0,lifeTime*1e9,1,0,true);
                if(f.side==LEFT){
                    ofSetColor(0,0,255,alpha*255);
                }else if(f.side==RIGHT){
                    ofSetColor(255,0,0,alpha*255);
                }else{ 
                    ofSetColor(127,127,127,alpha*255);
                }
                drawFoot(f.pos,f.dir,0.2);            
            }
        }
    }

    ofNoFill();
    if(!footHistory.empty()){
        uint64_t now=footHistory.rbegin()->first;
        //履歴の足を描画
        for(auto & fh:footHistory){
            for(auto & f:fh.second){
                float alpha=ofMap(now-fh.first,0,lifeTime*1e9,1,0,true);
                ofSetColor(255,0,0,alpha*50);
                ofDrawCircle(f.pos, 0.1);
            }
        }
        //最新の足を描画
        for(auto & f:footHistory.rbegin()->second){
            ofSetColor(0,255,0);
            ofDrawCircle(f.pos, 0.1);
            ofSetColor(255,255,0);
            ofDrawLine(f.pos,f.pos+f.vel*0.1);
        }
        //連続とみなす足の軌跡を描画
        for(auto & f:footHistory.rbegin()->second){
            const Foot * cur = &f;
            float alpha=1;
            while(true){
                if(cur->prevFootAddress.first==0)break;
                auto it = footHistory.find(cur->prevFootAddress.first);
                if(it==footHistory.end())break;
                auto & prevFoot = it->second[cur->prevFootAddress.second];
                ofSetColor(255,alpha*255);
                ofDrawLine(cur->pos,prevFoot.pos);
                cur = &prevFoot;
                alpha*=0.5;
            }
        }
    }

    ofPopStyle();
    ofPopMatrix();

    gui.begin();
    ImGui::SetWindowPos(ImVec2(600,100));
    ImGui::Text("%s",ofToString(ofGetFrameRate()).c_str());
    ImGui::DragFloat("Life Time [s]", &lifeTime, 0.1f, 0.1f, 10.0f);
    ImGui::DragFloat("Match Window [s]", &matchWindow, 0.01f, 0.03f, 1.0f);
    ImGui::DragFloat("Match Max Dist [m]", &matchMaxDist, 0.01f, 0.1f, 3.0f);
    ImGui::DragFloat("Landing Speed [m/s]", &landingSpeed, 0.01f, 0.05f, 5.0f);
    ImGui::DragFloat("Direction Window [s]", &directionWindow, 0.01f, 0.05f, 1.0f);
    ImGui::DragFloat("Stamp Life Time [s]", &stampLifeTime, 0.1f, 0.1f, 10.0f);
    ImGui::DragFloat("Prev Stamp Window [s]", &prevStampWindow, 0.1f, 0.1f, 10.0f);
    ImGui::DragFloat("Prev Stamp Min Dist [m]", &prevStampMinDist, 0.01f, 0.0f, 2.0f);
    ImGui::DragFloat("Prev Stamp Max Dist [m]", &prevStampMaxDist, 0.01f, 0.0f, 3.0f);
    ImGui::DragFloat("Blocker Radius [m]", &blockerRadius, 0.01f, 0.01f, 1.0f);
    ImGui::DragFloat("Blocker Life Time [s]", &blockerLifeTime, 0.01f, 0.0f, 1.0f);

    //送信先(Enterで反映)
    char ipBuf[64];
    strncpy(ipBuf,distIp.c_str(),sizeof(ipBuf)-1);
    ipBuf[sizeof(ipBuf)-1]='\0';
    if(ImGui::InputText("Dist IP",ipBuf,sizeof(ipBuf),ImGuiInputTextFlags_EnterReturnsTrue)){
        distIp=ipBuf;
        sender.setup(distIp, distPort);
    }
    if(ImGui::InputInt("Dist Port",&distPort,0,0,ImGuiInputTextFlags_EnterReturnsTrue)){
        sender.setup(distIp, distPort);
    }

    //受信ポート(Resetで反映)
    ImGui::InputInt("Listen Port",&listenPort,0,0);
    ImGui::SameLine();
    if(ImGui::Button("Reset"))rev.setup(listenPort);

    if(ImGui::Button("Save"))saveConfig();
    ImGui::SameLine();
    if(ImGui::Button("Load")){
        loadConfig();
        sender.setup(distIp, distPort);//送信先が変わっている場合に備えて再設定する
    }
    gui.end();
}

void ofApp::drawFoot(glm::vec2 _pos, glm::vec2 _dir, float _size){
    ofPushMatrix();
    ofTranslate(_pos);
    ofRotateDeg(atan2(_dir.y,_dir.x)*180/PI+90);
    ofDrawEllipse(0,0,_size*0.5,_size);
    ofDrawEllipse(0,_size*0.6,_size*0.4,_size*0.4);
    ofPopMatrix();
}

const ofApp::Foot & ofApp::findOldestLinkedFoot(const Foot & _start,float _window){
    const Foot * cur = &_start;
    while(true){
        if(cur->prevFootAddress.first==0)break;
        auto it = footHistory.find(cur->prevFootAddress.first);
        if(it==footHistory.end())break;
        if(_start.timeStamp-cur->prevFootAddress.first>_window*1e9)break;
        cur = &it->second[cur->prevFootAddress.second];
    }
    return *cur;
}

template<typename T>
pair<uint64_t,int> ofApp::findNearestInWindow(
    const map<uint64_t,vector<T>> & _history,
    glm::vec2 _pos, 
    uint64_t _now,
    float _window, 
    float _minDist, 
    float _maxDist
){
    float minDist2=numeric_limits<float>::max();
    pair<uint64_t,int> nearest=make_pair(0,-1);
    for(auto it=_history.rbegin();it!=_history.rend();++it){
        if(it->first >= _now)continue;
        if(_now-it->first>_window*1e9)break;
        for(int i=0;i<it->second.size();++i){
            float dist2=glm::length2(_pos-it->second[i].pos);
            if(dist2<minDist2&&dist2<_maxDist*_maxDist&&dist2>_minDist*_minDist){
                minDist2=dist2;
                nearest=make_pair(it->first,i);
            }
        }
    }
    return nearest;
}

void ofApp::sendFootStampOSC(const FootStamp & _fs){
    ofxOscMessage msg;
    msg.setAddress("/footstamp");
    // msg.addInt64Arg(ofGetElapsedTimeMicros());
    msg.addFloatArg(_fs.pos.x);
    msg.addFloatArg(_fs.pos.y);
    msg.addFloatArg(_fs.dir.x);
    msg.addFloatArg(_fs.dir.y);
    if(_fs.side!=UNDEFINED){
        msg.addInt32Arg(_fs.side);
    }
    else msg.addInt32Arg(LEFT);
    sender.sendMessage(msg,false);
}

//--------------------------------------------------------------
void ofApp::exit(){

}
